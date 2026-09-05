import * as anchor from "@anchor-lang/core";
import { Program } from "@anchor-lang/core";
import { NcVault } from "../target/types/nc_vault";
import { Keypair, LAMPORTS_PER_SOL, PublicKey, SystemProgram } from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  createMint,
  getAssociatedTokenAddressSync,
  getOrCreateAssociatedTokenAccount,
  mintTo,
  getAccount,
  getMint,
  transfer,
} from "@solana/spl-token";
import { BN } from "bn.js";
import { expect } from "chai";

const DECIMALS = 6;
const ONE = 1_000_000;

describe("nc-vault", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.ncVault as Program<NcVault>;
  const connection = provider.connection;
  const payer = (provider.wallet as any).payer as Keypair;

  const manager = Keypair.generate();
  const alice = Keypair.generate();
  const bob = Keypair.generate();

  let underlyingMint: PublicKey;
  let positionMint: PublicKey;
  let vault: PublicKey;
  let sharesMint: PublicKey;
  let vaultUnderlying: PublicKey;
  let vaultPosition: PublicKey;

  const ata = (mint: PublicKey, owner: PublicKey, offCurve = false) =>
    getAssociatedTokenAddressSync(mint, owner, offCurve);

  const amountOf = async (acct: PublicKey) => Number((await getAccount(connection, acct)).amount);

  const fund = async (kp: Keypair, sol: number) => {
    const sig = await connection.requestAirdrop(kp.publicKey, sol * LAMPORTS_PER_SOL);
    const bh = await connection.getLatestBlockhash();
    await connection.confirmTransaction({ signature: sig, ...bh }, "confirmed");
  };

  const deposit = (who: Keypair, amount: number) =>
    program.methods
      .deposit(new BN(amount))
      .accountsStrict({
        user: who.publicKey,
        vault,
        underlyingMint,
        sharesMint,
        userUnderlying: ata(underlyingMint, who.publicKey),
        userShares: ata(sharesMint, who.publicKey),
        vaultUnderlying,
        vaultPosition,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([who])
      .rpc();

  const withdraw = (who: Keypair, shares: number) =>
    program.methods
      .withdraw(new BN(shares))
      .accountsStrict({
        user: who.publicKey,
        vault,
        underlyingMint,
        positionMint,
        sharesMint,
        userShares: ata(sharesMint, who.publicKey),
        userUnderlying: ata(underlyingMint, who.publicKey),
        userPosition: ata(positionMint, who.publicKey),
        vaultUnderlying,
        vaultPosition,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([who])
      .rpc();

  const investIx = (name: "invest" | "divest", amount: number) =>
    program.methods[name](new BN(amount))
      .accountsStrict({
        manager: manager.publicKey,
        vault,
        underlyingMint,
        positionMint,
        managerUnderlying: ata(underlyingMint, manager.publicKey),
        managerPosition: ata(positionMint, manager.publicKey),
        vaultUnderlying,
        vaultPosition,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([manager])
      .rpc();

  before(async () => {
    for (const kp of [manager, alice, bob]) await fund(kp, 10);

    underlyingMint = await createMint(connection, payer, payer.publicKey, null, DECIMALS);
    positionMint = await createMint(connection, payer, payer.publicKey, null, DECIMALS);

    [vault] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), manager.publicKey.toBuffer(), underlyingMint.toBuffer()],
      program.programId
    );
    [sharesMint] = PublicKey.findProgramAddressSync(
      [Buffer.from("shares"), vault.toBuffer()],
      program.programId
    );
    vaultUnderlying = ata(underlyingMint, vault, true);
    vaultPosition = ata(positionMint, vault, true);

    // everyone needs their token accounts up front
    for (const kp of [manager, alice, bob]) {
      await getOrCreateAssociatedTokenAccount(connection, payer, underlyingMint, kp.publicKey);
      await getOrCreateAssociatedTokenAccount(connection, payer, positionMint, kp.publicKey);
    }

    await mintTo(connection, payer, underlyingMint, ata(underlyingMint, alice.publicKey), payer, 5000 * ONE);
    await mintTo(connection, payer, underlyingMint, ata(underlyingMint, bob.publicKey), payer, 5000 * ONE);
    // the manager needs a stack of position tokens to hand over every time they invest
    await mintTo(connection, payer, positionMint, ata(positionMint, manager.publicKey), payer, 5000 * ONE);
  });

  it("initializes with an empty vault and no shares", async () => {
    await program.methods
      .initialize()
      .accountsStrict({
        manager: manager.publicKey,
        underlyingMint,
        positionMint,
        vault,
        sharesMint,
        vaultUnderlying,
        vaultPosition,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([manager])
      .rpc();

    expect(Number((await getMint(connection, sharesMint)).supply)).to.equal(0);
    expect(await amountOf(vaultUnderlying)).to.equal(0);
    expect(await amountOf(vaultPosition)).to.equal(0);
  });

  it("first deposit gets one share per token", async () => {
    await deposit(alice, 1000 * ONE);

    expect(await amountOf(ata(sharesMint, alice.publicKey))).to.equal(1000 * ONE);
    expect(await amountOf(vaultUnderlying)).to.equal(1000 * ONE);
  });

  it("invest moves every last token into the position, so the vault has no cash", async () => {
    await investIx("invest", 1000 * ONE);

    expect(await amountOf(vaultUnderlying)).to.equal(0);
    expect(await amountOf(vaultPosition)).to.equal(1000 * ONE);
  });

  it("alice still exits with zero liquidity, paid in position tokens", async () => {
    const cashBefore = await amountOf(ata(underlyingMint, alice.publicKey));

    await withdraw(alice, 1000 * ONE);

    // the vault had no cash, so she gets the position instead. the exit still went through
    expect(await amountOf(ata(positionMint, alice.publicKey))).to.equal(1000 * ONE);
    expect(await amountOf(ata(underlyingMint, alice.publicKey))).to.equal(cashBefore);
    expect(Number((await getMint(connection, sharesMint)).supply)).to.equal(0);
    expect(await amountOf(vaultPosition)).to.equal(0);
  });

  it("splits both piles pro rata when the vault is half cash half position", async () => {
    await deposit(alice, 400 * ONE);
    await deposit(bob, 400 * ONE);
    await investIx("invest", 400 * ONE); // 400 cash, 400 position

    const bobShares = await amountOf(ata(sharesMint, bob.publicKey));
    const cashBefore = await amountOf(ata(underlyingMint, bob.publicKey));
    const posBefore = await amountOf(ata(positionMint, bob.publicKey));

    await withdraw(bob, bobShares / 2);

    // quarter of the whole vault: a quarter of each pile
    expect(await amountOf(ata(underlyingMint, bob.publicKey))).to.equal(cashBefore + 100 * ONE);
    expect(await amountOf(ata(positionMint, bob.publicKey))).to.equal(posBefore + 100 * ONE);
  });

  it("a plain token transfer into the vault raises the share price", async () => {
    const before = await amountOf(ata(sharesMint, bob.publicKey));

    // anyone can send tokens straight to the vault's ATA, no instruction needed
    await transfer(
      connection,
      payer,
      ata(underlyingMint, bob.publicKey),
      vaultUnderlying,
      bob,
      200 * ONE
    );

    await deposit(bob, 100 * ONE);

    // same tokens in, fewer shares out, because each share is worth more now
    const minted = (await amountOf(ata(sharesMint, bob.publicKey))) - before;
    expect(minted).to.be.lessThan(100 * ONE);
    expect(minted).to.be.greaterThan(0);
  });

  it("rejects a deposit too small to be worth a single share", async () => {
    try {
      await deposit(bob, 1);
      expect.fail("minted zero shares for a real deposit");
    } catch (err) {
      expect(err.toString()).to.include("ZeroShares");
    }
  });

  it("wont let anyone but the manager invest", async () => {
    try {
      await program.methods
        .invest(new BN(ONE))
        .accountsStrict({
          manager: alice.publicKey,
          vault,
          underlyingMint,
          positionMint,
          managerUnderlying: ata(underlyingMint, alice.publicKey),
          managerPosition: ata(positionMint, alice.publicKey),
          vaultUnderlying,
          vaultPosition,
          associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        })
        .signers([alice])
        .rpc();
      expect.fail("someone else invested the vault's money");
    } catch (err) {
      expect(err.toString()).to.match(/ConstraintSeeds|ConstraintHasOne|AnchorError/);
    }
  });
});
