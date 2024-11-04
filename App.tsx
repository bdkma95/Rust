import { WalletMultiButton } from '@solana/wallet-adapter-react-ui';
import { useMemo } from 'react';
import { useState } from 'react';
import { ConnectionProvider, WalletProvider } from '@solana/wallet-adapter-react';
import { WalletModalProvider } from '@solana/wallet-adapter-react-ui';
import { PhantomWalletAdapter } from '@solana/wallet-adapter-wallets';
import { clusterApiUrl } from '@solana/web3.js';
import BettingInterface from './components/BettingInterface';
import { 
  PhantomWalletAdapter,
  SolflareWalletAdapter,
  TorusWalletAdapter,
} from '@solana/wallet-adapter-wallets';

require('@solana/wallet-adapter-react-ui/styles.css');

function App() {
  // Define the endpoint for connecting to the Solana cluster
  const [network, setNetwork] = useState('devnet');
  const endpoint = useMemo(() => clusterApiUrl(network), [network]);
  
  // Add error handling
  const [error, setError] = useState(null);
  
  // Define the wallets that can be used
  const wallets = useMemo(() => [new PhantomWalletAdapter(), new SolflareWalletAdapter(), new TorusWalletAdapter(),], []);

  return (
    <ConnectionProvider endpoint={endpoint}>
        <WalletProvider wallets={wallets} autoConnect onError={(error) => setError(error.message)}>
            <WalletModalProvider>
                <div className="App">
                    <header>
                        <div className="header-left">
                            <h1>Solana Betting dApp</h1>
                            <select 
                                value={network} 
                                onChange={(e) => setNetwork(e.target.value)}
                            >
                                <option value="devnet">Devnet</option>
                                <option value="testnet">Testnet</option>
                                <option value="mainnet-beta">Mainnet Beta</option>
                            </select>
                        </div>
                        <WalletMultiButton />
                    </header>
                    {error && (
                        <div className="error-message">
                            {error}
                            <button onClick={() => setError(null)}>✕</button>
                        </div>
                    )}
                    <BettingInterface />
                </div>
            </WalletModalProvider>
        </WalletProvider>
    </ConnectionProvider>
  );
}

export default App;
