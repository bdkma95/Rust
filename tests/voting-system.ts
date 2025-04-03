import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { VotingSystem } from "../target/types/voting_system";
import { 
  PublicKey, 
  Keypair, 
  SYSVAR_CLOCK_PUBKEY, 
  SYSVAR_RENT_PUBKEY 
} from '@solana/web3.js';
import { 
  TOKEN_PROGRAM_ID, 
  createMint, 
  createAccount, 
  mintTo 
} from '@solana/spl-token';
import { assert } from "chai";

describe("voting-system", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.VotingSystem as Program<VotingSystem>;
  const admin = provider.wallet.payer;
  
  let governance: PublicKey;
  let tokenMint: PublicKey;
  let voterTokenAccount: PublicKey;
  let voter = Keypair.generate();

  const config = {
    maxTitleLength: 100,
    maxDescriptionLength: 500,
    minVotingDuration: new anchor.BN(3600), // 1 hour
    maxVotingDuration: new anchor.BN(2592000), // 30 days
    minTokenBalance: new anchor.BN(1000000),
    maxProposals: new anchor.BN(10),
    tokenDecimals: 6,
  };

  before(async () => {
    // Initialize token mint
    [tokenMint] = await PublicKey.findProgramAddressSync(
      [Buffer.from("governance-mint")],
      program.programId
    );

    // Create voter token account
    voterTokenAccount = await createAccount(
      provider.connection,
      admin,
      tokenMint,
      voter.publicKey
    );

    // Mint tokens to voter
    await mintTo(
      provider.connection,
      admin,
      tokenMint,
      voterTokenAccount,
      admin,
      config.minTokenBalance.toNumber()
    );
  });

  it("Initializes governance", async () => {
    [governance] = await PublicKey.findProgramAddressSync(
      [Buffer.from("governance")],
      program.programId
    );

    await program.methods.initialize(config)
      .accounts({
        governance,
        admin: admin.publicKey,
        tokenMint,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const governanceAccount = await program.account.governance.fetch(governance);
    assert.isFalse(governanceAccount.paused);
    assert.equal(governanceAccount.tokenMint.toBase58(), tokenMint.toBase58());
  });

  describe("Proposal Management", () => {
    let proposal: PublicKey;
    const title = "Test Proposal";
    const description = "This is a test proposal";
    const duration = new anchor.BN(86400); // 1 day

    it("Creates a proposal", async () => {
      const [proposalPda] = await PublicKey.findProgramAddressSync(
        [Buffer.from("proposal"), new anchor.BN(0).toArrayLike(Buffer, "le", 8)],
        program.programId
      );

      await program.methods.createProposal(title, description, duration)
        .accounts({
          governance,
          proposal: proposalPda,
          payer: admin.publicKey,
          admin: admin.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();

      proposal = proposalPda;
      const proposalAccount = await program.account.proposal.fetch(proposal);
      assert.equal(proposalAccount.title, title);
      assert.equal(proposalAccount.voteCount, 0);
    });

    it("Fails to create proposal with invalid duration", async () => {
      try {
        await program.methods.createProposal(
          title, 
          description, 
          new anchor.BN(3600 * 24 * 31) // 31 days
        )
        .accounts({ governance, proposal, admin: admin.publicKey })
        .rpc();
        assert.fail("Should have failed");
      } catch (err) {
        assert.include(err.message, "InvalidDuration");
      }
    });
  });

  describe("Voting System", () => {
    let proposal: PublicKey;
    let voteMarker: PublicKey;

    before(async () => {
      [proposal] = await PublicKey.findProgramAddressSync(
        [Buffer.from("proposal"), new anchor.BN(0).toArrayLike(Buffer, "le", 8)],
        program.programId
      );
    });

    it("Allows valid vote", async () => {
      const [votePda] = await PublicKey.findProgramAddressSync(
        [
          Buffer.from("vote"),
          proposal.toBuffer(),
          voter.publicKey.toBuffer(),
          new anchor.BN(0).toArrayLike(Buffer, "le", 8)
        ],
        program.programId
      );

      await program.methods.vote()
        .accounts({
          governance,
          proposal,
          voteMarker: votePda,
          voter: voter.publicKey,
          voterToken: voterTokenAccount,
          tokenProgram: TOKEN_PROGRAM_ID,
          clock: SYSVAR_CLOCK_PUBKEY,
        })
        .signers([voter])
        .rpc();

      voteMarker = votePda;
      const proposalAccount = await program.account.proposal.fetch(proposal);
      assert.equal(proposalAccount.voteCount, 1);
    });

    it("Prevents double voting", async () => {
      try {
        await program.methods.vote()
          .accounts({
            governance,
            proposal,
            voteMarker,
            voter: voter.publicKey,
            voterToken: voterTokenAccount,
          })
          .signers([voter])
          .rpc();
        assert.fail("Should have failed");
      } catch (err) {
        assert.include(err.message, "Account already initialized");
      }
    });
  });

  describe("Vote Closure", () => {
    let proposal: PublicKey;
    let voteMarker: PublicKey;

    before(async () => {
      [proposal] = await PublicKey.findProgramAddressSync(
        [Buffer.from("proposal"), new anchor.BN(0).toArrayLike(Buffer, "le", 8)],
        program.programId
      );

      [voteMarker] = await PublicKey.findProgramAddressSync(
        [
          Buffer.from("vote"),
          proposal.toBuffer(),
          voter.publicKey.toBuffer(),
          new anchor.BN(0).toArrayLike(Buffer, "le", 8)
        ],
        program.programId
      );

      // Advance clock past voting period
      await program.methods.advanceClock(new anchor.BN(86400 * 2)) // 2 days
        .accounts({ clock: SYSVAR_CLOCK_PUBKEY })
        .rpc();
    });

    it("Closes vote account successfully", async () => {
      await program.methods.closeVote()
        .accounts({
          voteMarker,
          voter: voter.publicKey,
          proposal,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([voter])
        .rpc();

      try {
        await program.account.voteMarker.fetch(voteMarker);
        assert.fail("Account should be closed");
      } catch (err) {
        assert.include(err.message, "Account does not exist");
      }
    });
  });

  describe("Pause Functionality", () => {
    it("Allows admin to pause system", async () => {
      await program.methods.setPaused(true)
        .accounts({
          governance,
          admin: admin.publicKey,
        })
        .rpc();

      const governanceAccount = await program.account.governance.fetch(governance);
      assert.isTrue(governanceAccount.paused);
    });

    it("Prevents proposals when paused", async () => {
      try {
        await program.methods.createProposal(
          "Paused Proposal", 
          "Should fail", 
          new anchor.BN(3600)
        )
        .accounts({ governance })
        .rpc();
        assert.fail("Should have failed");
      } catch (err) {
        assert.include(err.message, "SystemPaused");
      }
    });
  });
});
