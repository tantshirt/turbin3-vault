# Vault

Turbin3 Q3 2026, week 2. Two programs in one workspace:

- **`programs/vault`** — a SOL vault. Deposit, withdraw, close. This is the assignment.
- **`programs/nc-vault`** — a non-custodial token vault where you can always get out,
  even when the vault has no cash left. This is the advanced extension challenge.

## Run it

```bash
pnpm install
anchor build
anchor test --validator legacy
```

`--validator legacy` matters. Anchor 1.0 defaults to Surfpool, and if you don't have
Surfpool installed the run dies with `Failed to spawn 'surfpool'`. The flag tells it to
use `solana-test-validator` instead.

15 tests, all passing. Screenshot is in `screenshots/`.

---

## Part 1: the SOL vault

Two PDAs per user:

| PDA | Seeds | What it is |
|-----|-------|------------|
| `vault_state` | `["state", user]` | Tiny account holding the two bumps |
| `vault` | `["vault", vault_state]` | A `SystemAccount` that just holds lamports |

The holding account is a `SystemAccount`, not an `Account`. It never stores data, only
lamports, so there's nothing to deserialize.

### Instructions

| Instruction | What it does |
|---|---|
| `initialize` | Creates `vault_state`, caches both bumps on it |
| `deposit(amount)` | User sends lamports to the vault |
| `withdraw(amount)` | Vault sends lamports back, signed by the vault PDA |
| `close` | Empties the vault and closes `vault_state` |

### Why there's no `has_one = owner`

Most vault tutorials store the owner on the state account and check it with `has_one`.
This one doesn't, and it's on purpose.

The seeds are `["state", user.key()]` where `user` is the signer. Anchor re-derives that
address from whoever signed and compares. If an attacker signs and passes someone else's
`vault_state`, the derivation doesn't match and the constraint fails before the handler
runs. Storing a 32 byte owner field and checking it a second time protects against
nothing extra, so it's not there. There's a test for this.

### The rent boundary

A system account can't drop below the rent-exempt minimum. If it does the transfer fails
with a runtime error that tells you nothing useful, so the program checks first:

```rust
let min = Rent::get()?.minimum_balance(0);
require!(self.vault.lamports().saturating_sub(amount) >= min, VaultError::InsufficientFunds);
```

**The comparison is `>=`.** Landing on exactly the rent-exempt minimum is fine. One
lamport under is not. Both are tested.

`saturating_sub` instead of plain `-` because the release profile has `overflow-checks = true`,
so subtracting more than the balance would panic instead of returning a clean error.

---

## Part 2: the non-custodial vault

### The idea in one paragraph

The vault holds two token accounts: **cash** (the underlying token) and a **position**
token. You deposit cash and get **share tokens** back, priced against the total of both
accounts. To leave, you burn shares and the program hands you your exact percentage of
*each* account. If the manager has moved all the cash into the position, you get position
tokens instead of cash and your withdrawal still goes through. There is no instruction
that lets the manager send assets to themselves, no pause switch, and no lockup. That's
what makes it non-custodial and exit-anytime.

### What the position token is

One position token means one unit of underlying that has been put to work in some market.
It's a stand-in for an LP token or a market receipt. Defining it at par like that buys
three things:

1. **No oracle needed.** `total_assets = cash_balance + position_balance`. Exact, always.
2. **`invest` can't be a rug.** It takes one `amount` and uses it for both legs, so an
   uneven swap isn't something you can even express in the instruction.
3. **Testing "no liquidity" is one call.** `invest(everything)` and the cash account is zero.

### Accounts

```rust
pub struct Vault {
    pub manager: Pubkey,          // can call invest/divest, and nothing else
    pub underlying_mint: Pubkey,
    pub position_mint: Pubkey,
    pub shares_mint: Pubkey,
    pub bump: u8,
}
```

That's the whole state. Everything else is derived and checked by constraints:

| Account | Seeds / derivation |
|---|---|
| `vault` | `["vault", manager, underlying_mint]` |
| `shares_mint` | `["shares", vault]`, mint authority is the vault PDA |
| `vault_underlying` | ATA of `underlying_mint`, owned by the vault |
| `vault_position` | ATA of `position_mint`, owned by the vault |

### Instructions

| Instruction | What it does |
|---|---|
| `initialize` | Creates the config, the shares mint, and the vault's two token accounts |
| `deposit(amount)` | Takes your cash, mints you shares pro rata |
| `withdraw(shares)` | Burns your shares, pays you a slice of **both** accounts |
| `invest(amount)` | Manager swaps cash out for position tokens, 1:1 |
| `divest(amount)` | The other direction |

### Share math

On deposit:

```
shares = amount * supply / total_assets      (or just `amount` if supply is 0)
```

The balances get read before the incoming transfer lands, so `total_assets` is the
pre-deposit number. The code does the math first and transfers second for exactly that
reason.

On withdraw, both payouts come off the pre-burn supply:

```
underlying_out = cash_balance     * shares / supply
position_out   = position_balance * shares / supply
```

**`transfer_checked` with an amount of 0 is legal.** That's the trick that makes the
zero-liquidity exit work with no special case. If the cash account is empty, that leg
transfers 0 and the position leg pays out everything you're owed. Same code path either way.

### Rounding

All three divisions truncate. Every one of them rounds **against whoever is acting** and
in favor of everyone still in the pool:

- Depositor gets fewer shares, the remainder accrues to existing holders.
- Withdrawer gets slightly less of each asset, the dust stays.

A vault that rounded the other way could be drained by looping tiny deposits and
withdrawals. One clean case: if you're the only depositor, `shares == supply`, so
`balance * supply / supply` is exact and you get everything back. That's tested.

### The first-depositor attack, and why it's only half fixed

An attacker deposits 1 unit so `supply == 1`, then sends a big pile of tokens straight
into the vault's token account. Anyone can do that, no instruction required. Now
`total_assets` is huge, and the next person's `amount * 1 / total_assets` truncates to 0
shares. They'd hand over real tokens and get nothing.

`require!(shares > 0, ZeroShares)` is in the code. It turns "you lose your money" into
"your transaction fails and you keep your money." That's the part worth having.

The full fix is a virtual share offset (what ERC-4626 does with `_decimalsOffset`), or
minting a small batch of dead shares on the first deposit. Both work. Both add a magic
constant to the share math that makes it harder to read, and this is a teaching build,
so it's documented here instead of implemented.

Related: because share price is read off live balances, anyone sending tokens to the
vault raises the price for everyone. That's not a bug, it's the mechanism that lets a
market pay yield with a plain transfer and no extra instruction.

### What's deliberately not here

| Left out | Why |
|---|---|
| Oracle / position pricing | Position tokens are par, so total assets is just a sum |
| A real DeFi integration | Would bury the vault logic this is meant to show |
| Management or performance fees | Extra surface area, teaches nothing new here |
| Virtual shares | Documented above, `require!(shares > 0)` covers the damage |
| Withdrawal queue or cooldown | The whole point is exit anytime, a queue contradicts that |
| Pause switch | A pause is custodial by definition, it would make the headline false |

---

## Tests

```
anchor test --validator legacy
```

### vault (7)

1. initialize, bumps get cached
2. deposit, vault goes up by exactly the amount
3. withdraw back to the owner
4. someone else tries to withdraw → **fails**
5. withdraw down to exactly rent-exempt → passes, this is the `>=` boundary
6. one lamport past it → **fails** with `InsufficientFunds`
7. close, state account gone, lamports returned

### nc-vault (8)

1. initialize, empty vault, zero shares
2. first deposit, one share per token
3. invest everything, vault cash hits zero
4. **alice exits anyway and gets paid in position tokens** — this is the whole extension
5. half cash half position, partial withdraw gets a slice of both
6. plain token transfer into the vault raises the share price
7. deposit too small to earn a share → **fails** with `ZeroShares`
8. someone who isn't the manager tries to invest → **fails**

Four of these tests pass by failing. Tests 4, 6, 7 in the vault list and 7, 8 in the
nc-vault list are checking that the program rejects something. A green check on those
means the rejection worked.

## Notes on Anchor 1.0

Two things that cost real time here and aren't in older tutorials:

**`CpiContext::new` takes a `Pubkey` now, not an `AccountInfo`.** Every 0.3x snippet you
copy will fail to compile:

```rust
CpiContext::new(self.token_program.to_account_info(), accounts)  // 0.3x
CpiContext::new(self.token_program.key(), accounts)              // 1.x
```

**`anchor build -p <name>` creates a second `target/` inside the program folder** and
generates a fresh keypair there, which then doesn't match `declare_id!`. Plain
`anchor build` is fine. If you see a program ID mismatch out of nowhere, look for a
stray `programs/<name>/target/`.
