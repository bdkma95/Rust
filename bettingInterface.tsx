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
  
  const connection = new Connection('https://api.mainnet-beta.solana.com'); // Use appropriate cluster
  const provider = new AnchorProvider(connection, { publicKey }, { commitment: 'confirmed' });
  const program = new Program(IDL, new PublicKey('YourProgramIdHere'), provider);

  useEffect(() => {
    if (publicKey) {
      fetchUserProfile();
      fetchBetPools();
    }
  }, [publicKey]);

  const fetchUserProfile = async () => {
    try {
      const userProfileAccount = await getUserProfileAccount(publicKey);
      const profileData = await program.account.userProfile.fetch(userProfileAccount);
      setUserProfile(profileData);
    } catch (error) {
      console.error("Error fetching user profile:", error);
    }
  };

  const fetchBetPools = async () => {
    try {
      const pools = await program.account.betPool.all(); // Fetch all bet pools
      setBetPools(pools.map(pool => ({
        publicKey: pool.publicKey,
        ...pool.account,
      })));
    } catch (error) {
      console.error("Error fetching bet pools:", error);
    }
  };

  const createUserProfile = async () => {
    try {
      await program.methods.createUserProfile()
        .accounts({
          user: publicKey,
          userProfile: await getUserProfileAccount(publicKey),
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();
      
      alert("User profile created successfully!");
      fetchUserProfile(); // Refresh user profile after creation
    } catch (error) {
      console.error("Error creating user profile:", error);
    }
  };

  const placeBet = async (poolId: string, outcome: string) => {
    try {
      await program.methods.placeBet(new anchor.BN(betAmount), outcome)
        .accounts({
          pool: new PublicKey(poolId),
          user: publicKey,
          userProfile: await getUserProfileAccount(publicKey),
        })
        .rpc();

      alert("Bet placed successfully!");
      fetchUserProfile(); // Update user stats after placing a bet
      fetchBetPools(); // Refresh bet pools if necessary
    } catch (error) {
      console.error("Error placing bet:", error);
    }
  };

  // Helper function to get the user's profile account address
  const getUserProfileAccount = async (userPubkey: PublicKey): Promise<PublicKey> => {
    return (await PublicKey.findProgramAddress(
      [userPubkey.toBuffer()],
      program.programId
    ))[0];
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
          <p>Total Bets: {userProfile.totalBets.toString()}</p>
          <p>Total Wins: {userProfile.totalWins.toString()}</p>
        </div>
      )}

      <div className="betting-pools">
        <h2>Active Betting Pools</h2>
        {betPools.map((pool) => (
          <div key={pool.publicKey.toString()} className="pool-card">
            <h3>Outcome: {pool.outcome}</h3>
            <p>Current Odds: {pool.odds.toString()}</p>
            <p>Total Bets: {pool.totalBets.toString()}</p>
            <div className="place-bet">
              <input
                type="number"
                value={betAmount}
                onChange={(e) => setBetAmount(e.target.value)}
                placeholder="Bet amount"
              />
              <button onClick={() => placeBet(pool.publicKey.toString(), "Win")}>
                Bet on Win
              </button>
              <button onClick={() => placeBet(pool.publicKey.toString(), "Lose")}>
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
