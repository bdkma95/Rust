import { Program, AnchorProvider } from '@project-serum/anchor';
import { Connection, PublicKey, Transaction } from '@solana/web3.js';
import { IDL } from '../idl/betting';

export class BettingService {
  private program: Program;
  private connection: Connection;

  constructor(provider: AnchorProvider) {
    this.connection = provider.connection;
    this.program = new Program(IDL, new PublicKey('YourProgramIdHere'), provider);
  }

  // Create a new user profile
  async createUserProfile(userPubkey: PublicKey) {
    const tx = await this.program.methods
      .createUserProfile()
      .accounts({
        user: userPubkey,
        userProfile: await this.getUserProfileAccount(userPubkey),
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    console.log("User profile created with transaction:", tx);
  }

  // Get user profile by public key
  async getUserProfile(userPubkey: PublicKey) {
    const userProfileAccount = await this.getUserProfileAccount(userPubkey);
    const userProfileData = await this.program.account.userProfile.fetch(userProfileAccount);
    
    return userProfileData;
  }

  // Place a bet on a specific pool
  async placeBet(poolPubkey: PublicKey, amount: number, outcome: string, userPubkey: PublicKey) {
    const tx = await this.program.methods
      .placeBet(new anchor.BN(amount), outcome)
      .accounts({
        pool: poolPubkey,
        user: userPubkey,
        userProfile: await this.getUserProfileAccount(userPubkey),
      })
      .rpc();

    console.log("Bet placed with transaction:", tx);
  }

  // Fetch all betting pools
  async getBetPools() {
    const pools = await this.connection.getProgramAccounts(this.program.programId);
    
    return pools.map(pool => ({
      pubkey: pool.pubkey,
      account: this.program.account.betPool.fetch(pool.pubkey),
    }));
  }

  // Helper function to get the user's profile account address
  private async getUserProfileAccount(userPubkey: PublicKey): Promise<PublicKey> {
    return (await PublicKey.findProgramAddress(
      [userPubkey.toBuffer()],
      this.program.programId
    ))[0];
  }
}
