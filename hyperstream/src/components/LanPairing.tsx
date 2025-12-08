import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import QRCode from 'react-qr-code';

export const LanPairing: React.FC = () => {
    const [localIp, setLocalIp] = useState<string>('');
    const [pairingCode, setPairingCode] = useState<string>('');
    const [qrData, setQrData] = useState<string>('');
    const [port] = useState<number>(14733); // Default port from backend

    useEffect(() => {
        loadData();
    }, []);

    const loadData = async () => {
        try {
            const ip = await invoke<string>('get_local_ip');
            setLocalIp(ip);
            generateNewCode();
        } catch (e) {
            console.error('Failed to load LAN info:', e);
        }
    };

    const generateNewCode = async () => {
        try {
            const code = await invoke<string>('generate_lan_pairing_code');
            setPairingCode(code);
            const qr = await invoke<string>('get_lan_pairing_qr_data', { port, code });
            setQrData(qr);
        } catch (e) {
            console.error('Failed to generate pairing code:', e);
        }
    };

    return (
        <div className="bg-gray-800 p-6 rounded-lg shadow-lg max-w-md mx-auto">
            <h2 className="text-xl font-bold mb-4 text-white">Mobile Pairing</h2>

            <div className="flex flex-col items-center space-y-4">
                <div className="bg-white p-4 rounded-lg">
                    {qrData ? (
                        <QRCode value={qrData} size={200} />
                    ) : (
                        <div className="w-[200px] h-[200px] bg-gray-200 animate-pulse rounded"></div>
                    )}
                </div>

                <div className="text-center">
                    <p className="text-gray-400 text-sm mb-1">Pairing Code</p>
                    <div className="text-3xl font-mono font-bold tracking-widest text-blue-400">
                        {pairingCode || '....'}
                    </div>
                </div>

                <div className="text-center text-sm text-gray-400">
                    <p>Local IP: <span className="text-white font-mono">{localIp || 'Loading...'}</span></p>
                    <p>Port: <span className="text-white font-mono">{port}</span></p>
                </div>

                <button
                    onClick={generateNewCode}
                    className="px-4 py-2 bg-gray-700 hover:bg-gray-600 rounded text-white text-sm transition-colors"
                >
                    Generate New Code
                </button>

                <p className="text-xs text-gray-500 text-center max-w-xs">
                    Scan this QR code with the HyperStream Mobile app to control downloads remotely.
                </p>
            </div>
        </div>
    );
};
