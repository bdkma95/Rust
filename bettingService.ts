import { Program, AnchorProvider } from '@project-serum/anchor';
import { Connection, PublicKey } from '@solana/web3.js';
import { IDL } from '../idl/betting';

export class BettingService {
  private program: Program;
  private connection: Connection;

  constructor(provider: AnchorProvider) {
    this.connection = provider.connection;
    this.program = new Program(IDL, new PublicKey('YourProgramIdHere'), provider);
  }

  async createUserProfile() {
    // Implement create user profile
  }

  async getUserProfile(userPubkey: PublicKey) {
    // Implement get user profile
  }

  async placeBet(poolPubkey: PublicKey, amount: number, outcome: string) {
    // Implement place bet
  }

  async getBetPools() {
    // Implement get bet pools
  }
}
