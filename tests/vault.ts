import * as anchor from "@anchor-lang/core";
import { Program } from "@anchor-lang/core";
import { Vault } from "../target/types/vault";
import { Keypair, LAMPORTS_PER_SOL, PublicKey, SystemProgram } from "@solana/web3.js";
import { BN } from "bn.js";
import { expect } from "chai";

describe("vault", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.vault as Program<Vault>;
  const connection = provider.connection;

  // fresh user every run so the PDAs are always empty to start
  const user = Keypair.generate();
  const attacker = Keypair.generate();

  const [vaultState] = PublicKey.findProgramAddressSync(
    [Buffer.from("state"), user.publicKey.toBuffer()],
    program.programId
  );
  const [vault] = PublicKey.findProgramAddressSync(
    [Buffer.from("vault"), vaultState.toBuffer()],
    program.programId
  );

  // lamports a 0 byte system account needs to stay alive
  let rentMin: number;

  const fund = async (kp: Keypair, sol: number) => {
    const sig = await connection.requestAirdrop(kp.publicKey, sol * LAMPORTS_PER_SOL);
    const bh = await connection.getLatestBlockhash();
    await connection.confirmTransaction({ signature: sig, ...bh }, "confirmed");
  };

  before(async () => {
    await fund(user, 5);
    await fund(attacker, 2);
    rentMin = await connection.getMinimumBalanceForRentExemption(0);
  });

  it("initializes and caches both bumps", async () => {
    await program.methods
      .initialize()
      .accountsStrict({
        user: user.publicKey,
        vaultState,
        vault,
        systemProgram: SystemProgram.programId,
      })
      .signers([user])
      .rpc();

    const state = await program.account.vaultState.fetch(vaultState);
    const [, expectedVaultBump] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), vaultState.toBuffer()],
      program.programId
    );
    expect(state.vaultBump).to.equal(expectedVaultBump);
  });

  it("deposits and the vault goes up by exactly that much", async () => {
    const before = await connection.getBalance(vault);
    const amount = 2 * LAMPORTS_PER_SOL;

    await program.methods
      .deposit(new BN(amount))
      .accountsStrict({
        user: user.publicKey,
        vaultState,
        vault,
        systemProgram: SystemProgram.programId,
      })
      .signers([user])
      .rpc();

    expect(await connection.getBalance(vault)).to.equal(before + amount);
  });

  it("withdraws back to the owner", async () => {
    const vaultBefore = await connection.getBalance(vault);
    const userBefore = await connection.getBalance(user.publicKey);
    const amount = 0.5 * LAMPORTS_PER_SOL;

    await program.methods
      .withdraw(new BN(amount))
      .accountsStrict({
        user: user.publicKey,
        vaultState,
        vault,
        systemProgram: SystemProgram.programId,
      })
      .signers([user])
      .rpc();

    expect(await connection.getBalance(vault)).to.equal(vaultBefore - amount);
    // user pays the fee too so just check it went up
    expect(await connection.getBalance(user.publicKey)).to.be.greaterThan(userBefore);
  });

  it("wont let someone else drain the vault", async () => {
    try {
      await program.methods
        .withdraw(new BN(0.1 * LAMPORTS_PER_SOL))
        .accountsStrict({
          user: attacker.publicKey,
          vaultState, // the victim's state account
          vault,
          systemProgram: SystemProgram.programId,
        })
        .signers([attacker])
        .rpc();
      expect.fail("attacker withdrew from someone else's vault");
    } catch (err) {
      // seeds are derived from the signer, so this state PDA doesn't match
      expect(err.toString()).to.match(/ConstraintSeeds|AnchorError/);
    }
  });

  it("allows draining down to exactly rent exempt", async () => {
    const balance = await connection.getBalance(vault);
    const amount = balance - rentMin;

    await program.methods
      .withdraw(new BN(amount))
      .accountsStrict({
        user: user.publicKey,
        vaultState,
        vault,
        systemProgram: SystemProgram.programId,
      })
      .signers([user])
      .rpc();

    // check is `remaining >= rentMin`, so landing exactly on it is allowed
    expect(await connection.getBalance(vault)).to.equal(rentMin);
  });

  it("rejects one lamport past the rent exempt line", async () => {
    try {
      await program.methods
        .withdraw(new BN(1))
        .accountsStrict({
          user: user.publicKey,
          vaultState,
          vault,
          systemProgram: SystemProgram.programId,
        })
        .signers([user])
        .rpc();
      expect.fail("withdrew past rent exempt");
    } catch (err) {
      expect(err.toString()).to.include("InsufficientFunds");
    }
  });

  it("closes the vault and gives everything back", async () => {
    const userBefore = await connection.getBalance(user.publicKey);
    const vaultBefore = await connection.getBalance(vault);

    await program.methods
      .close()
      .accountsStrict({
        user: user.publicKey,
        vaultState,
        vault,
        systemProgram: SystemProgram.programId,
      })
      .signers([user])
      .rpc();

    expect(await connection.getBalance(vault)).to.equal(0);
    expect(await connection.getAccountInfo(vaultState)).to.equal(null);
    // vault lamports plus the state account's rent, minus the tx fee
    expect(await connection.getBalance(user.publicKey)).to.be.greaterThan(userBefore + vaultBefore);
  });
});
