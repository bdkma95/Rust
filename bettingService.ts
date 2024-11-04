import { Program, AnchorProvider, web3 } from '@project-serum/anchor';
import { Connection, PublicKey, Transaction } from '@solana/web3.js';
import { TOKEN_PROGRAM_ID, getAssociatedTokenAddress } from '@solana/spl-token';
import { IDL } from '../idl/betting';

export class BettingService {
  private program: Program;
  private connection: Connection;

  constructor(provider: AnchorProvider) {
    this.connection = provider.connection;
    this.program = new Program(IDL, new PublicKey('YourProgramIdHere'), provider);
  }

  async createUserProfile(userPubkey: PublicKey) {
    const [userProfilePda] = await this.getUserProfilePDA(userPubkey);

    const tx = await this.program.methods
      .createUserProfile()
      .accounts({
        userProfile: userProfilePda,
        user: userPubkey,
        systemProgram: web3.SystemProgram.programId,
      })
      .rpc();

    return tx;
  }

  async createBettingPool(adminPubkey: PublicKey, outcome: string) {
    const [poolPda] = await this.getBetPoolPDA(outcome);
    
    const tx = await this.program.methods
      .createBettingPool(outcome)
      .accounts({
        betPool: poolPda,
        admin: adminPubkey,
        systemProgram: web3.SystemProgram.programId,
      })
      .rpc();

    return tx;
  }

  async placeBet(
    userPubkey: PublicKey, 
    poolPubkey: PublicKey, 
    amount: number,
    mint: PublicKey // Token mint address
  ) {
    const userTokenAccount = await getAssociatedTokenAddress(mint, userPubkey);
    const poolTokenAccount = await getAssociatedTokenAddress(mint, poolPubkey);
    const [userProfilePda] = await this.getUserProfilePDA(userPubkey);

    const tx = await this.program.methods
      .placeBet(new web3.BN(amount))
      .accounts({
        user: userPubkey,
        userTokenAccount,
        betPoolTokenAccount: poolTokenAccount,
        betPool: poolPubkey,
        userProfile: userProfilePda,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();

    return tx;
  }

  async resolveBets(
    adminPubkey: PublicKey,
    poolPubkey: PublicKey,
    winningOutcome: string,
    mint: PublicKey
  ) {
    const poolTokenAccount = await getAssociatedTokenAddress(mint, poolPubkey);
