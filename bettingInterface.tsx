import { useWallet, useConnection } from '@solana/wallet-adapter-react';
import { useState, useEffect } from 'react';
import { Program, AnchorProvider, BN } from '@project-serum/anchor';
import { Connection, PublicKey, Transaction } from '@solana/web3.js';
import { TOKEN_PROGRAM_ID, getAssociatedTokenAddress } from '@solana/spl-token';
import { IDL } from '../idl/betting';

const BettingInterface = () => {
  const { publicKey, sendTransaction } = useWallet();
  const { connection } = useConnection();
  const [userProfile, setUserProfile] = useState(null);
  const [betPools, setBetPools] = useState([]);
  const [betAmount, setBetAmount] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState('');
  
  // Initialize program
  const provider = new AnchorProvider(
    connection,
    window.solana,
    { commitment: 'confirmed' }
  );
  const program = new Program(IDL, new PublicKey('YourProgramIdHere'), provider);

  // Constants
  const TOKEN_MINT = new PublicKey('YourTokenMintAddress');
  
  useEffect(() => {
    if (publicKey) {
      fetchUserProfile();
      fetchBetPools();
    }
  }, [publicKey]);

  const fetchUserProfile = async () => {
    try {
      setIsLoading(true);
      const [profilePda] = await PublicKey.findProgramAddress(
        [Buffer.from('user_profile'), publicKey.toBuffer()],
        program.programId
      );
      
      const profileData = await program.account.userProfile.fetch(profilePda);
      setUserProfile(profileData);
    } catch (error) {
      console.error("Error fetching user profile:", error);
      setError('Failed to fetch user profile');
    } finally {
      setIsLoading(false);
    }
  };

  const fetchBetPools = async () => {
    try {
      setIsLoading(true);
      const pools = await program.account.betPool.all();
      setBetPools(pools.map(pool => ({
        publicKey: pool.publicKey,
        totalBets: pool.account.totalBets.toString(),
        odds: pool.account.odds,
        outcome: pool.account.outcome,
        bets: pool.account.bets,
      })));
    } catch (error) {
      console.error("Error fetching bet pools:", error);
      setError('Failed to fetch betting pools');
    } finally {
      setIsLoading(false);
    }
  };

  const createUserProfile = async () => {
    try {
      setIsLoading(true);
      const [profilePda] = await PublicKey.findProgramAddress(
        [Buffer.from('user_profile'), publicKey.toBuffer()],
        program.programId
      );

      await program.methods.createUserProfile()
        .accounts({
          userProfile: profilePda,
          user: publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();
      
      await fetchUserProfile();
    } catch (error) {
      console.error("Error creating user profile:", error);
      setError('Failed to create user profile');
    } finally {
      setIsLoading(false);
    }
  };

  const placeBet = async (poolPubkey: PublicKey, outcome: string) => {
    try {
      setIsLoading(true);
      setError('');

      const userTokenAccount = await getAssociatedTokenAddress(
        TOKEN_MINT,
        publicKey
      );
      
      const poolTokenAccount = await getAssociatedTokenAddress(
        TOKEN_MINT,
        poolPubkey
      );

      const [profilePda] = await PublicKey.findProgramAddress(
        [Buffer.from('user_profile'), publicKey.toBuffer()],
        program.programId
      );

      await program.methods.placeBet(new BN(betAmount))
        .accounts({
          user: publicKey,
          userTokenAccount,
          betPoolTokenAccount: poolTokenAccount,
          betPool: poolPubkey,
          userProfile: profilePda,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .rpc();

      await Promise.all([
        fetchUserProfile(),
        fetchBetPools()
      ]);
      
      setBetAmount('');
    } catch (error) {
      console.error("Error placing bet:", error);
      setError('Failed to place bet');
    } finally {
      setIsLoading(false);
    }
  };

  if (!publicKey) {
    return <div className="connect-wallet-prompt">Please connect your wallet to continue</div>;
  }

  return (
    <div className="betting-interface">
      {error && (
        <div className="error-message">
          {error}
          <button onClick={() => setError('')}>✕</button>
        </div>
      )}
      
      {isLoading && <div className="loading-spinner">Loading...</div>}

      {!userProfile ? (
        <button 
          onClick={createUserProfile}
          disabled={isLoading}
          className="create-profile-button"
        >
          Create Profile
        </button>
      ) : (
        <div className="user-stats">
          <h2>Your Stats</h2>
          <p>Total Bets: {userProfile.totalBets.toString()}</p>
          <p>Total Wins: {userProfile.totalWins.toString()}</p>
          <div className="betting-history">
            <h3>Betting History</h3>
            {userProfile.bettingHistory.map((bet, index) => (
              <div key={index} className="bet-history-item">
                <span>Amount: {bet.amount.toString()}</span>
                <span>Outcome: {bet.outcome}</span>
              </div>
            ))}
          </div>
        </div>
      )}

      <div className="betting-pools">
        <h2>Active Betting Pools</h2>
        {betPools.map((pool) => (
          <div key={pool.publicKey.toString()} className="pool-card">
            <h3>Outcome: {pool.outcome}</h3>
            <p>Current Odds: {pool.odds.toFixed(2)}x</p>
            <p>Total Bets: {pool.totalBets} tokens</p>
            <div className="place-bet">
              <input
                type="number"
                value={betAmount}
                onChange={(e) => setBetAmount(e.target.value)}
                placeholder="Bet amount in tokens"
                min="0"
                step="1"
                disabled={isLoading}
              />
              <button 
                onClick={() => placeBet(pool.publicKey, "Win")}
                disabled={isLoading || !betAmount}
              >
                Bet on Win
              </button>
              <button 
                onClick={() => placeBet(pool.publicKey, "Lose")}
                disabled={isLoading || !betAmount}
              >
                Bet on Lose
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};

export default BettingInterface;
