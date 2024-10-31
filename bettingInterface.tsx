import { useWallet } from '@solana/wallet-adapter-react';
import { useState, useEffect } from 'react';
import { Program, AnchorProvider } from '@project-serum/anchor';
import { Connection, PublicKey } from '@solana/web3.js';
import { IDL } from '../idl/betting'; // You'll need to generate this

const BettingInterface = () => {
  const { publicKey, sendTransaction } = useWallet();
  const [userProfile, setUserProfile] = useState(null);
  const [betPools, setBetPools] = useState([]);
  const [betAmount, setBetAmount] = useState('');
  
  useEffect(() => {
    if (publicKey) {
      fetchUserProfile();
      fetchBetPools();
    }
  }, [publicKey]);

  const fetchUserProfile = async () => {
    // Implement fetching user profile using the program
  };

  const fetchBetPools = async () => {
    // Implement fetching active bet pools
  };

  const createUserProfile = async () => {
    // Implement create user profile logic
  };

  const placeBet = async (poolId: string, outcome: string) => {
    // Implement place bet logic
  };

  if (!publicKey) {
    return <div>Please connect your wallet to continue</div>;
  }

  return (
    <div className="betting-interface">
      {!userProfile ? (
        <button onClick={createUserProfile}>Create Profile</button>
      ) : (
        <div className="user-stats">
          <h2>Your Stats</h2>
          <p>Total Bets: {userProfile.totalBets}</p>
          <p>Total Wins: {userProfile.totalWins}</p>
        </div>
      )}

      <div className="betting-pools">
        <h2>Active Betting Pools</h2>
        {betPools.map((pool) => (
          <div key={pool.publicKey} className="pool-card">
            <h3>Outcome: {pool.outcome}</h3>
            <p>Current Odds: {pool.odds}</p>
            <p>Total Bets: {pool.totalBets}</p>
            <div className="place-bet">
              <input
                type="number"
                value={betAmount}
                onChange={(e) => setBetAmount(e.target.value)}
                placeholder="Bet amount"
              />
              <button onClick={() => placeBet(pool.publicKey, "Win")}>
                Bet on Win
              </button>
              <button onClick={() => placeBet(pool.publicKey, "Lose")}>
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
