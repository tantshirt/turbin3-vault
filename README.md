# Vault

Week 2 for Turbin3. Two programs in here:

- `programs/vault` is the SOL vault the assignment asked for. Deposit, withdraw, close.
- `programs/nc-vault` is the advanced extension. A token vault you can always get out
  of, even when there's no cash left in it.

## Running it

```bash
pnpm install
anchor build
anchor test --validator legacy
```

You need `--validator legacy`. Anchor 1.0 goes for Surfpool by default and I don't have
Surfpool installed, so without the flag it just dies with `Failed to spawn 'surfpool'`.
Took me way too long to work out that's all it was.

15 tests pass. Screenshot is in `screenshots/`.

---

## The SOL vault

Every user gets two PDAs. `vault_state` at `["state", user]` holds two bumps and that's
it. `vault` at `["vault", vault_state]` is a `SystemAccount` that sits there holding
lamports. It never stores data so there's nothing to deserialize, which is why it's a
`SystemAccount` and not an `Account`.

Four instructions:

- `initialize` makes the state account and saves both bumps on it
- `deposit` sends lamports in
- `withdraw` sends them back, signed by the vault PDA
- `close` empties the vault and closes the state account

### No has_one, on purpose

Most vault examples save the owner on the state account and check it with `has_one`.
I didn't do that.

The seeds are `["state", user.key()]` and `user` is the signer. Anchor rederives that
address from whoever signed and compares. So if somebody signs with their own key and
passes my `vault_state`, the address doesn't match and it fails before my code even
runs. Storing a 32 byte owner and checking it a second time doesn't stop anything the
seeds aren't already stopping. There's a test where an attacker tries exactly this.

### The rent thing

System accounts can't drop below rent exempt. If you try, the transfer fails with a
runtime error that tells you nothing useful. So I check first:

```rust
let min = Rent::get()?.minimum_balance(0);
require!(self.vault.lamports().saturating_sub(amount) >= min, VaultError::InsufficientFunds);
```

It's `>=`, so landing on exactly the rent exempt minimum is fine and one lamport under
isn't. Both are tested.

`saturating_sub` instead of plain subtraction because the release profile has overflow
checks on, so `lamports() - amount` would panic instead of giving back my error.

---

## The non-custodial vault

### What it actually does

The vault holds two token accounts. One is cash, the other is a position token. You put
cash in and get share tokens back, priced against both accounts added together. When you
want out you burn shares and the program hands you your percentage of each account.

The interesting part is what happens when there's no cash. If the manager moved it all
into the position, you get position tokens instead and your withdrawal still goes
through. There's no instruction that lets the manager send anything to themselves, no
pause button, no lockup. That's the non-custodial bit.

### The position token

One position token means one unit of underlying that's been put to work somewhere. It's
standing in for an LP token. I defined it at par on purpose because it makes three
things easy:

- total assets is just cash + position, so no oracle
- `invest` takes one amount and uses it for both legs, so you can't even write a call
  that swaps unevenly
- getting to zero liquidity in a test is one `invest(everything)`

### State

```rust
pub struct Vault {
    pub manager: Pubkey,
    pub underlying_mint: Pubkey,
    pub position_mint: Pubkey,
    pub shares_mint: Pubkey,
    pub bump: u8,
}
```

That's all of it. The vault is at `["vault", manager, underlying_mint]`, the shares mint
is at `["shares", vault]` with the vault PDA as its mint authority, and the two token
accounts are just ATAs the vault owns.

Instructions are `initialize`, `deposit`, `withdraw`, `invest`, `divest`. The manager can
only call invest and divest.

### The math

Deposit:

```
shares = amount * supply / total_assets      (or just amount if supply is 0)
```

I read the balances before the transfer goes through, so `total_assets` is the number
from before your deposit landed. That's the reason the math happens first and the
transfer second, not the other way round.

Withdraw, both worked out off the supply before the burn:

```
underlying_out = cash     * shares / supply
position_out   = position * shares / supply
```

Here's the thing that makes the empty vault case work: `transfer_checked` with an amount
of 0 is legal. So if there's no cash, that leg moves 0 and the position leg pays you
everything you're owed. Same code either way, no if statement, no special case.

### Rounding

All three divisions truncate and they all round against whoever's calling. A depositor
gets slightly fewer shares, a withdrawer gets slightly less of each asset, and the
leftover stays with everyone still in the pool. If it rounded the other way you could
drain the thing with a loop of tiny deposits and withdrawals.

If you're the only one in, `shares == supply`, so `balance * supply / supply` is exact
and you get all of it back. That's a test.

### The first depositor attack

Somebody deposits 1 unit so supply is 1, then sends a big pile of tokens straight to the
vault's token account. Anyone can do that, no instruction needed. Now total assets is
huge, and the next person's `amount * 1 / total_assets` rounds down to 0 shares. They'd
hand over real tokens and get nothing back.

I've got `require!(shares > 0, ZeroShares)` in there. That turns "you lose your money"
into "your transaction fails and you keep your money", which is the part that matters.

The proper fix is virtual shares, same idea as ERC-4626. It works, but it sticks a magic
number in the middle of the share math and makes it harder to read, so I left it out and
wrote it down here instead.

Side effect of pricing off live balances: anyone sending tokens to the vault raises the
share price for everyone. Not a bug. That's how a market can pay yield without needing
another instruction.

### Left out on purpose

No oracle, since position tokens are par and total assets is just addition. No real DeFi
integration, it would bury the vault logic this is supposed to show. No fees. No
withdrawal queue or cooldown, because the assignment says exit anytime and a queue is
the opposite of that. No pause switch, because a pause is custodial by definition and
would make this whole section a lie.

---

## Tests

**vault (7):** initialize, deposit, withdraw, someone else tries to withdraw and fails,
withdraw down to exactly rent exempt, one lamport past it fails, close.

**nc-vault (8):** initialize, first deposit, invest everything so there's no cash left,
alice gets out anyway and gets paid in position tokens, half and half vault splits pro
rata, a plain transfer into the vault moves the share price, a deposit too small to earn
a share fails, someone who isn't the manager tries to invest and fails.

That fourth nc-vault test is the whole extension. Everything else is supporting it.

Five of the 15 pass by failing. They're checking the program says no to something. A
green check there means it said no.

---

## Two things that ate my afternoon

`CpiContext::new` wants a `Pubkey` in Anchor 1.x, not an `AccountInfo`. Every example I
found is written for 0.3x so nothing compiles until you swap it:

```rust
CpiContext::new(self.token_program.to_account_info(), accounts)  // old
CpiContext::new(self.token_program.key(), accounts)              // 1.x
```

`anchor build -p <name>` makes a whole second `target/` inside the program folder with a
brand new keypair in it, and then that keypair doesn't match your `declare_id!`. I got a
program ID mismatch out of nowhere and couldn't work out where the address was even
coming from. Just use plain `anchor build`.
