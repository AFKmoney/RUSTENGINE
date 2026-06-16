'use client';

import { useEffect, useState, useRef, useCallback } from 'react';
import type { Socket } from 'socket.io-client';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Badge } from '@/components/ui/badge';

// ═══════════════════════════════════════════════════════════
// VORTEX PRIME — Frontend v3
// ═══════════════════════════════════════════════════════════

type TabId = 'analyser' | 'avalanche' | 'fractal' | 'resonance' | 'inversion';

interface LogEntry {
  type: 'info' | 'success' | 'warning' | 'error';
  msg: string;
  time: string;
}

interface AnalysisResult {
  success: boolean;
  target: { pubkey?: string; address?: string; sha256: string; hash160: string; verified?: boolean };
  pipeline: { pubkey?: string; sha256: string; hash160: string; address?: string; verified?: boolean | null };
  fractal: any;
  avalanche: { wall: number };
  range: { puzzleNum: number; nMin: string; nMax: string; rangeSize: string };
}

export default function VortexPrime() {
  const [socket, setSocket] = useState<Socket | null>(null);
  const [connected, setConnected] = useState(false);
  const [activeTab, setActiveTab] = useState<TabId>('analyser');
  const [pubkey, setPubkey] = useState('');
  const [hash, setHash] = useState('');
  const [address, setAddress] = useState('');
  const [puzzleNum, setPuzzleNum] = useState(135);
  const [strategy, setStrategy] = useState('all');
  const [analysis, setAnalysis] = useState<AnalysisResult | null>(null);
  const [running, setRunning] = useState(false);
  const [found, setFound] = useState(false);
  const [privateKey, setPrivateKey] = useState('');
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [stats, setStats] = useState({ iterations: 0, kangarooSteps: 0, incrementalSteps: 0, elapsed: 0, keysPerSec: '0' });
  const logRef = useRef<HTMLDivElement>(null);
  const canvasRefs = useRef<Record<string, HTMLCanvasElement | null>>({});

  // Connect socket.io
  useEffect(() => {
    let s: Socket;
    (async () => {
      const { io } = await import('socket.io-client');
      s = io('/?XTransformPort=3003', {
        transports: ['websocket', 'polling'],
        forceNew: true, reconnection: true, reconnectionAttempts: 10, reconnectionDelay: 2000, timeout: 15000
      });
      setSocket(s);

    s.on('connect', () => { setConnected(true); addLog('success', 'Backend connecté'); });
    s.on('disconnect', () => { setConnected(false); addLog('warning', 'Backend déconnecté'); });

    s.on('analysis-result', (data: AnalysisResult) => {
      setAnalysis(data);
      addLog('success', `Analyse terminée — Puzzle #${data.range.puzzleNum} — ${data.fractal.topAnomalies?.length || 0} anomalies`);
      setTimeout(() => drawAllCharts(data), 100);
    });

    s.on('inversion-started', (data: any) => {
      setRunning(true); setFound(false);
      addLog('success', `Inversion lancée — Puzzle #${data.puzzleNum} — ${data.strategy}`);
    });

    s.on('progress', (data: any) => {
      if (data.type === 'kangaroo') {
        addLog('info', `[Kangaroo ${data.phase || ''}] Step ${(data.step || 0).toLocaleString()} | ${data.rate || 0} steps/s${data.traps ? ` | Traps: ${data.traps.toLocaleString()}` : ''}`);
      } else if (data.type === 'incremental') {
        addLog('info', `[Incremental] ${(data.step || 0).toLocaleString()} / ${(data.count || 0).toLocaleString()} | ${data.rate || 0} keys/s`);
      } else if (data.type === 'fractal-guided') {
        addLog('info', `[Fractal] ${(data.checked || 0).toLocaleString()} | ${data.rate || 0} keys/s`);
      }
    });

    s.on('found', (data: any) => {
      setFound(true); setPrivateKey(data.privateKey); setRunning(false);
      addLog('success', `★★★ CLÉ PRIVÉE TROUVÉE ★★★ — ${data.privateKey} (${data.strategy})`);
    });

    s.on('inversion-stopped', () => { setRunning(false); addLog('warning', 'Inversion arrêtée'); });
    s.on('inversion-complete', () => { setRunning(false); addLog('info', 'Inversion terminée'); });
    s.on('error', (data: any) => { addLog('error', data.message); });
    })();

    return () => { if (s) s.disconnect(); };
  }, []);

  // Periodic stats update
  useEffect(() => {
    if (!running) return;
    const interval = setInterval(() => {
      // Request status from socket
      if (socket?.connected) {
        socket.emit('get-stats');
      }
    }, 2000);
    return () => clearInterval(interval);
  }, [running, socket]);

  const addLog = useCallback((type: LogEntry['type'], msg: string) => {
    const time = new Date().toLocaleTimeString();
    setLogs(prev => [...prev.slice(-200), { type, msg, time }]);
  }, []);

  useEffect(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight;
  }, [logs]);

  // ── Actions ──
  const handleAnalyze = () => {
    if (!pubkey && !hash && !address) { addLog('error', 'Entrez au moins une pubkey, hash ou adresse'); return; }
    socket?.emit('analyze', { pubkey: pubkey || undefined, hash: hash || undefined, address: address || undefined, puzzleNum });
    addLog('info', 'Analyse en cours via backend...');
  };

  const handleStartInversion = () => {
    if (!analysis) { addLog('error', 'Analysez une cible d\'abord'); return; }
    socket?.emit('start-inversion', { puzzleNum, strategy });
  };

  const handleStop = () => {
    socket?.emit('stop-inversion');
  };

  const handleReset = () => {
    setPubkey(''); setHash(''); setAddress(''); setAnalysis(null); setFound(false); setPrivateKey('');
    setRunning(false); setLogs([]); setStats({ iterations: 0, kangarooSteps: 0, incrementalSteps: 0, elapsed: 0, keysPerSec: '0' });
  };

  // ── Chart Drawing ──
  const drawAllCharts = (data: AnalysisResult) => {
    drawSignatureRadar(data.fractal);
    drawBoxCounting(data.fractal.boxCounting);
    drawWalsh(data.fractal.walshHadamard);
    drawSelfSim(data.fractal.selfSimData);
    drawResonance(data.fractal.resonance);
  };

  const getCanvas = (id: string) => canvasRefs.current[id];

  const drawSignatureRadar = (fr: any) => {
    const canvas = getCanvas('signatureRadar'); if (!canvas) return;
    const ctx = canvas.getContext('2d'); if (!ctx) return;
    const W = canvas.width, H = canvas.height;
    ctx.clearRect(0, 0, W, H); ctx.fillStyle = '#0a0e17'; ctx.fillRect(0, 0, W, H);
    const cx = W / 2, cy = H / 2 + 10, R = 90;
    const labels = ['Dim Fract.', 'Plat. Spect.', 'Auto-Sim.', 'Anomalies', 'Ronds Faib.', 'Biais Spect.'];
    const rawValues = [fr.dimension || 0, fr.spectralFlatness || 0, fr.selfSimilarity || 0, (fr.topAnomalies || []).length / 10, (fr.anomalyRounds || []).length / 8, (fr.biasedWords || []).length / 8];
    const maxVals = [256, 100, 1, 1, 1, 1];
    const values = rawValues.map((v: number, i: number) => Math.min(1, v / maxVals[i]));
    ctx.strokeStyle = '#1a2540'; ctx.lineWidth = 1;
    for (let ring = 1; ring <= 4; ring++) { const r = R * ring / 4; ctx.beginPath(); for (let i = 0; i <= 6; i++) { const a = (Math.PI * 2 * i / 6) - Math.PI / 2; ctx.lineTo(cx + Math.cos(a) * r, cy + Math.sin(a) * r); } ctx.stroke(); }
    for (let i = 0; i < 6; i++) { const a = (Math.PI * 2 * i / 6) - Math.PI / 2; ctx.strokeStyle = '#1a2540'; ctx.beginPath(); ctx.moveTo(cx, cy); ctx.lineTo(cx + Math.cos(a) * R, cy + Math.sin(a) * R); ctx.stroke(); ctx.fillStyle = '#5a6580'; ctx.font = '9px monospace'; ctx.textAlign = 'center'; ctx.fillText(labels[i], cx + Math.cos(a) * (R + 20), cy + Math.sin(a) * (R + 20) + 3); }
    ctx.beginPath();
    for (let i = 0; i <= 6; i++) { const idx = i % 6; const a = (Math.PI * 2 * idx / 6) - Math.PI / 2; const r = R * values[idx]; ctx.lineTo(cx + Math.cos(a) * r, cy + Math.sin(a) * r); }
    ctx.closePath(); ctx.fillStyle = 'rgba(0, 255, 136, 0.15)'; ctx.fill(); ctx.strokeStyle = '#00ff88'; ctx.lineWidth = 2; ctx.stroke();
  };

  const drawBoxCounting = (bc: any) => {
    const canvas = getCanvas('boxCounting'); if (!canvas || !bc?.scales) return;
    const ctx = canvas.getContext('2d'); if (!ctx) return;
    const W = canvas.width, H = canvas.height;
    ctx.clearRect(0, 0, W, H); ctx.fillStyle = '#0a0e17'; ctx.fillRect(0, 0, W, H);
    const pad = { top: 30, right: 20, bottom: 40, left: 70 };
    const pW = W - pad.left - pad.right, pH = H - pad.top - pad.bottom;
    const logS = bc.scales.map((s: number) => Math.log2(s)), logC = bc.counts.map((c: number) => Math.log2(Math.max(1, c)));
    const minX = Math.min(...logS), maxX = Math.max(...logS), minY = Math.min(...logC), maxY = Math.max(...logC);
    const rX = maxX - minX || 1, rY = maxY - minY || 1;
    ctx.strokeStyle = '#ff6600'; ctx.lineWidth = 2; ctx.beginPath();
    for (let i = 0; i < logS.length; i++) {
      const x = pad.left + ((logS[i] - minX) / rX) * pW, y = pad.top + pH * (1 - (logC[i] - minY) / rY);
      i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y); ctx.fillStyle = '#ff6600'; ctx.fillRect(x - 3, y - 3, 6, 6);
    }
    ctx.stroke();
    if (bc.dimensions) { ctx.fillStyle = '#ff9944'; ctx.font = '10px monospace'; ctx.textAlign = 'left'; bc.dimensions.forEach((d: any, i: number) => ctx.fillText(`D ≈ ${d.dimension.toFixed(3)} (ε=${d.scale})`, pad.left + 10, pad.top + 20 + i * 15)); }
  };

  const drawWalsh = (wh: any) => {
    const canvas = getCanvas('walsh'); if (!canvas || !wh?.spectra) return;
    const ctx = canvas.getContext('2d'); if (!ctx) return;
    const W = canvas.width, H = canvas.height;
    ctx.clearRect(0, 0, W, H); ctx.fillStyle = '#0a0e17'; ctx.fillRect(0, 0, W, H);
    const colors = ['#00ff88','#00ccff','#ff6600','#ff4444','#aa44ff','#ffff00','#ff00ff','#00ffff'];
    const pad = { top: 30, right: 20, bottom: 40, left: 60 };
    const pW = W - pad.left - pad.right, pH = H - pad.top - pad.bottom;
    let gMax = 0, gMin = 0;
    for (const s of wh.spectra) for (const v of s.values) { if (v > gMax) gMax = v; if (v < gMin) gMin = v; }
    const range = gMax - gMin || 1;
    for (let w = 0; w < wh.spectra.length; w++) {
      const vals = wh.spectra[w].values;
      ctx.strokeStyle = colors[w % colors.length]; ctx.lineWidth = 1; ctx.globalAlpha = 0.7; ctx.beginPath();
      for (let i = 0; i < vals.length; i++) { const x = pad.left + (i / (vals.length - 1)) * pW; const y = pad.top + pH * (1 - (vals[i] - gMin) / range); i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y); }
      ctx.stroke();
    }
    ctx.globalAlpha = 1.0;
  };

  const drawSelfSim = (ss: any) => {
    const canvas = getCanvas('selfSim'); if (!canvas || !ss?.ratios) return;
    const ctx = canvas.getContext('2d'); if (!ctx) return;
    const W = canvas.width, H = canvas.height;
    ctx.clearRect(0, 0, W, H); ctx.fillStyle = '#0a0e17'; ctx.fillRect(0, 0, W, H);
    const pad = { top: 30, right: 20, bottom: 40, left: 60 };
    const pW = W - pad.left - pad.right, pH = H - pad.top - pad.bottom;
    const maxR = Math.max(...ss.ratios.map((r: any) => r.ratio), 1);
    const barW = pW / ss.ratios.length * 0.6;
    const meanR = ss.ratios.reduce((a: number, r: any) => a + r.ratio, 0) / ss.ratios.length;
    for (let i = 0; i < ss.ratios.length; i++) {
      const x = pad.left + (i + 0.5) * (pW / ss.ratios.length) - barW / 2;
      const h = (ss.ratios[i].ratio / maxR) * pH;
      const dev = Math.abs(ss.ratios[i].ratio - meanR);
      ctx.fillStyle = dev < 0.1 ? '#00ff88' : dev < 0.3 ? '#ffaa00' : '#ff4444';
      ctx.fillRect(x, pad.top + pH - h, barW, h);
    }
  };

  const drawResonance = (res: any) => {
    const canvas = getCanvas('resonance'); if (!canvas || !res?.matrix) return;
    const ctx = canvas.getContext('2d'); if (!ctx) return;
    const W = canvas.width, H = canvas.height;
    ctx.clearRect(0, 0, W, H); ctx.fillStyle = '#0a0e17'; ctx.fillRect(0, 0, W, H);
    const pad = { top: 40, right: 20, bottom: 60, left: 80 };
    const pW = W - pad.left - pad.right, pH = H - pad.top - pad.bottom;
    const scales = res.scales || [], matrix = res.matrix || [];
    if (!matrix.length || !scales.length) return;
    const cW = pW / scales.length, cH = pH / matrix.length;
    const maxA = res.maxAnomaly || 1;
    for (let r = 0; r < matrix.length; r++) {
      for (let s = 0; s < matrix[r].values.length; s++) {
        const val = matrix[r].values[s];
        const intensity = Math.min(1, val / Math.max(maxA, 3));
        let red: number, green: number, blue: number;
        if (intensity < 0.5) { red = Math.floor(intensity*2*255); green = Math.floor(intensity*2*200); blue = Math.floor((1-intensity*2)*100); }
        else { red = 255; green = Math.floor((1-(intensity-0.5)*2)*200); blue = 0; }
        ctx.fillStyle = `rgb(${red},${green},${blue})`;
        ctx.fillRect(pad.left + s * cW + 1, pad.top + r * cH + 1, cW - 2, cH - 2);
      }
    }
  };

  const tabs: { id: TabId; label: string }[] = [
    { id: 'analyser', label: 'ADRESSE & PUBKEY' },
    { id: 'fractal', label: 'FRACTALES' },
    { id: 'resonance', label: 'RÉSONANCE' },
    { id: 'inversion', label: 'INVERSION LIVE' },
  ];

  const logColor = (type: string) => {
    switch (type) {
      case 'success': return 'text-green-400';
      case 'error': return 'text-red-400';
      case 'warning': return 'text-yellow-400';
      default: return 'text-cyan-400';
    }
  };

  return (
    <div className="min-h-screen bg-[#060a12] text-[#c8d0e0] font-mono text-[13px]">
      {/* Header */}
      <header className="text-center py-5 border-b border-[#1a2540] mb-4">
        <h1 className="text-3xl font-black tracking-[8px]">
          <span className="text-[#00ff88]">V</span>ORTEX <span className="text-[#00ccff] text-xl align-super">PRIME</span>
        </h1>
        <p className="text-[11px] text-[#5a6580] tracking-[4px] mt-1">Discrete Fractal Inversion Engine — Backend v3</p>
      </header>

      <div className="max-w-[1400px] mx-auto px-4 pb-8">
        {/* Target Input */}
        <Card className="bg-[#0c1220] border-[#1a2540] mb-4">
          <CardHeader className="pb-2">
            <CardTitle className="text-[#00ff88] text-sm tracking-[3px]">CIBLE — Adresse / Pubkey / Hash</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
              <div>
                <label className="block text-[10px] text-[#5a6580] tracking-wider uppercase mb-1">Public Key (66/130 hex)</label>
                <Input value={pubkey} onChange={e => setPubkey(e.target.value)} placeholder="02... ou 03..." className="bg-[#060a12] border-[#1a2540] text-[#00ff88] font-mono text-xs" spellCheck={false} />
              </div>
              <div>
                <label className="block text-[10px] text-[#5a6580] tracking-wider uppercase mb-1">Hash SHA-256 (64 hex)</label>
                <Input value={hash} onChange={e => setHash(e.target.value)} placeholder="a1b2c3... 64 hex chars" className="bg-[#060a12] border-[#1a2540] text-[#00ff88] font-mono text-xs" spellCheck={false} />
              </div>
              <div>
                <label className="block text-[10px] text-[#5a6580] tracking-wider uppercase mb-1">Adresse Bitcoin</label>
                <Input value={address} onChange={e => setAddress(e.target.value)} placeholder="1... ou 3... ou bc1..." className="bg-[#060a12] border-[#1a2540] text-[#00ff88] font-mono text-xs" spellCheck={false} />
              </div>
            </div>
            <div className="grid grid-cols-1 md:grid-cols-[120px_1fr_200px] gap-3 items-end">
              <div>
                <label className="block text-[10px] text-[#5a6580] tracking-wider uppercase mb-1">Puzzle #</label>
                <Input type="number" value={puzzleNum} onChange={e => setPuzzleNum(parseInt(e.target.value) || 135)} min={1} max={256} className="bg-[#060a12] border-[#1a2540] text-[#ffaa00] font-mono text-lg font-black text-center" />
              </div>
              <div className="bg-[#060a12] border border-[#1a2540] rounded px-3 py-2 text-[#5a6580] text-xs tracking-wider">
                Range: [2<sup className="text-[#ffaa00] font-bold">{puzzleNum - 1}</sup>, 2<sup className="text-[#ffaa00] font-bold">{puzzleNum}</sup>) — 2<sup className="text-[#ffaa00] font-bold">{puzzleNum - 1}</sup> clés
              </div>
              <div>
                <label className="block text-[10px] text-[#5a6580] tracking-wider uppercase mb-1">Stratégie</label>
                <Select value={strategy} onValueChange={setStrategy}>
                  <SelectTrigger className="bg-[#060a12] border-[#1a2540] text-[#00ff88] font-mono text-xs">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent className="bg-[#0c1220] border-[#1a2540]">
                    <SelectItem value="all">Toutes (Kangaroo + Inc + Fractal)</SelectItem>
                    <SelectItem value="kangaroo">Pollard Kangaroo</SelectItem>
                    <SelectItem value="incremental">Incremental</SelectItem>
                    <SelectItem value="fractal-guided">Fractal-Guided</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </div>
            <div className="flex gap-2 flex-wrap items-center">
              <Button onClick={handleAnalyze} disabled={!connected} className="bg-[#00ff88] text-black font-bold hover:bg-[#33ffaa] hover:shadow-[0_0_20px_rgba(0,255,136,0.3)]">
                ANALYSER & INIT
              </Button>
              <Button onClick={handleStartInversion} disabled={!analysis || running || !connected} className="bg-[#00ccff] text-black font-bold hover:bg-[#33ddff]">
                LANCER INVERSION
              </Button>
              <Button onClick={handleStop} disabled={!running} className="bg-[#ff4444] text-white font-bold hover:bg-[#ff6666]">
                ARRÊTER
              </Button>
              <Button onClick={handleReset} variant="outline" className="border-[#1a2540] text-[#5a6580]">
                RESET
              </Button>
              <Badge className={`ml-auto ${connected ? 'bg-green-900/50 text-green-400 border-green-700' : 'bg-red-900/50 text-red-400 border-red-700'}`}>
                {connected ? 'BACKEND: CONNECTÉ' : 'BACKEND: DÉCONNECTÉ'}
              </Badge>
            </div>
          </CardContent>
        </Card>

        {/* Tabs */}
        <nav className="flex gap-0.5 mb-4 border-b border-[#1a2540]">
          {tabs.map(tab => (
            <button key={tab.id} onClick={() => setActiveTab(tab.id)}
              className={`px-5 py-2.5 font-bold text-[11px] tracking-[2px] uppercase border-b-2 transition-colors ${activeTab === tab.id ? 'text-[#00ff88] border-[#00ff88]' : 'text-[#5a6580] border-transparent hover:text-[#c8d0e0]'}`}>
              {tab.label}
            </button>
          ))}
        </nav>

        {/* Tab Content */}
        {activeTab === 'analyser' && (
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
            {/* Pipeline */}
            <Card className="lg:col-span-2 bg-[#0c1220] border-[#1a2540]">
              <CardHeader className="pb-2"><CardTitle className="text-[#00ccff] text-xs tracking-[2px]">Pipeline Bitcoin — Dérivation</CardTitle></CardHeader>
              <CardContent>
                <div className="flex flex-wrap items-center gap-2">
                  {[
                    { label: 'Public Key', value: analysis?.pipeline?.pubkey, cls: 'text-[#00ff88]' },
                    { label: 'SHA-256', value: analysis?.pipeline?.sha256, cls: 'text-[#00ccff]' },
                    { label: 'Hash160', value: analysis?.pipeline?.hash160, cls: 'text-[#00ccff]' },
                    { label: 'Adresse', value: analysis?.pipeline?.address, cls: 'text-[#ffaa00]' },
                  ].map((step, i) => (
                    <div key={i} className="flex items-center gap-2">
                      {i > 0 && <span className="text-[#5a6580] text-[10px]">→</span>}
                      <div className="bg-[#060a12] border border-[#1a2540] rounded-md p-2 min-w-[140px] text-center">
                        <div className="text-[8px] text-[#5a6580] tracking-wider uppercase">{step.label}</div>
                        <div className={`text-[10px] font-mono break-all ${step.cls}`}>{step.value ? (step.value.length > 24 ? step.value.slice(0,16) + '...' + step.value.slice(-8) : step.value) : '—'}</div>
                      </div>
                    </div>
                  ))}
                </div>
                {analysis?.pipeline?.verified === true && <p className="text-green-400 text-xs mt-2 font-bold">✓ Adresse vérifiée</p>}
                {analysis?.pipeline?.verified === false && <p className="text-red-400 text-xs mt-2 font-bold">✗ Adresse ne correspond PAS</p>}
              </CardContent>
            </Card>

            {/* Signature Fractale */}
            <Card className="lg:col-span-2 bg-[#0c1220] border-[#1a2540]">
              <CardHeader className="pb-2"><CardTitle className="text-[#00ccff] text-xs tracking-[2px]">Signature Fractale du Hash Cible</CardTitle></CardHeader>
              <CardContent>
                <div className="grid grid-cols-3 md:grid-cols-6 gap-2 mb-3">
                  {[
                    { label: 'Dim. Fractale', value: analysis?.fractal?.dimension?.toFixed(4) },
                    { label: 'Platitude Spect.', value: analysis?.fractal?.spectralFlatness?.toFixed(4) },
                    { label: 'Auto-Similarité', value: analysis?.fractal?.selfSimilarity?.toFixed(4) },
                    { label: 'Anomalies', value: analysis?.fractal?.topAnomalies?.length?.toString(), highlight: true },
                    { label: 'Rounds Faibles', value: analysis?.fractal?.anomalyRounds?.join(', ') || 'aucun', highlight: true },
                    { label: 'Biais Spectral', value: analysis?.fractal?.biasedWords?.length + ' mots' },
                  ].map((item, i) => (
                    <div key={i} className="bg-[#060a12] border border-[#1a2540] rounded p-2 text-center">
                      <div className="text-[7px] text-[#5a6580] tracking-wider uppercase">{item.label}</div>
                      <div className={`text-sm font-black ${item.highlight ? 'text-[#ffaa00]' : 'text-[#00ccff]'}`}>{item.value || '—'}</div>
                    </div>
                  ))}
                </div>
                <canvas ref={el => { canvasRefs.current['signatureRadar'] = el; }} width={400} height={250} className="w-full" />
              </CardContent>
            </Card>

            {/* Range Info */}
            <Card className="lg:col-span-2 bg-[#0c1220] border-[#1a2540]">
              <CardHeader className="pb-2"><CardTitle className="text-[#00ccff] text-xs tracking-[2px]">Range de Recherche — Puzzle #{analysis?.range?.puzzleNum || puzzleNum}</CardTitle></CardHeader>
              <CardContent>
                <div className="grid grid-cols-2 md:grid-cols-4 gap-2">
                  <div className="bg-[#060a12] border border-[#1a2540] rounded p-2 text-center">
                    <div className="text-[7px] text-[#5a6580] tracking-wider uppercase">Min (hex)</div>
                    <div className="text-xs font-mono text-[#00ff88] break-all">{analysis?.range?.nMin || '—'}</div>
                  </div>
                  <div className="bg-[#060a12] border border-[#1a2540] rounded p-2 text-center">
                    <div className="text-[7px] text-[#5a6580] tracking-wider uppercase">Max (hex)</div>
                    <div className="text-xs font-mono text-[#00ff88] break-all">{analysis?.range?.nMax || '—'}</div>
                  </div>
                  <div className="bg-[#060a12] border border-[#1a2540] rounded p-2 text-center">
                    <div className="text-[7px] text-[#5a6580] tracking-wider uppercase">Taille Range</div>
                    <div className="text-sm font-black text-[#ffaa00]">{analysis?.range?.rangeSize || `2^${puzzleNum-1}`}</div>
                  </div>
                  <div className="bg-[#060a12] border border-[#1a2540] rounded p-2 text-center">
                    <div className="text-[7px] text-[#5a6580] tracking-wider uppercase">Avalanche Wall</div>
                    <div className="text-sm font-black text-[#00ccff]">Round {analysis?.avalanche?.wall ?? '—'}</div>
                  </div>
                </div>
              </CardContent>
            </Card>
          </div>
        )}

        {activeTab === 'fractal' && (
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
            <Card className="bg-[#0c1220] border-[#1a2540]">
              <CardHeader className="pb-2"><CardTitle className="text-[#00ccff] text-xs tracking-[2px]">Box-Counting sur {`{0,1}²⁵⁶`}</CardTitle></CardHeader>
              <CardContent>
                <canvas ref={el => { canvasRefs.current['boxCounting'] = el; }} width={600} height={300} className="w-full" />
                <p className="text-[#5a6580] text-xs mt-2">Dimension estimée: <strong className="text-[#00ff88]">{analysis?.fractal?.dimension?.toFixed(4) || '—'}</strong></p>
              </CardContent>
            </Card>
            <Card className="bg-[#0c1220] border-[#1a2540]">
              <CardHeader className="pb-2"><CardTitle className="text-[#00ccff] text-xs tracking-[2px]">Spectre Walsh-Hadamard</CardTitle></CardHeader>
              <CardContent>
                <canvas ref={el => { canvasRefs.current['walsh'] = el; }} width={600} height={300} className="w-full" />
                <div className="flex gap-4 text-xs text-[#5a6580] mt-2">
                  <span>Platitude: <strong className="text-[#00ff88]">{analysis?.fractal?.spectralFlatness?.toFixed(4) || '—'}</strong></span>
                  <span>Non-linéarité: <strong className="text-[#00ff88]">{analysis?.fractal?.walshHadamard?.nonlinearity?.toFixed(1) || '—'}</strong></span>
                </div>
              </CardContent>
            </Card>
            <Card className="lg:col-span-2 bg-[#0c1220] border-[#1a2540]">
              <CardHeader className="pb-2"><CardTitle className="text-[#00ccff] text-xs tracking-[2px]">Auto-Similarité — Hamming</CardTitle></CardHeader>
              <CardContent>
                <canvas ref={el => { canvasRefs.current['selfSim'] = el; }} width={600} height={300} className="w-full" />
                <p className="text-[#5a6580] text-xs mt-2">Score: <strong className="text-[#00ff88]">{analysis?.fractal?.selfSimilarity?.toFixed(4) || '—'}</strong></p>
              </CardContent>
            </Card>
          </div>
        )}

        {activeTab === 'resonance' && (
          <div className="space-y-4">
            <Card className="bg-[#0c1220] border-[#1a2540]">
              <CardHeader className="pb-2"><CardTitle className="text-[#00ccff] text-xs tracking-[2px]">Scanner de Résonance — Anomalies (Round × Échelle)</CardTitle></CardHeader>
              <CardContent>
                <canvas ref={el => { canvasRefs.current['resonance'] = el; }} width={800} height={400} className="w-full" />
                <div className="flex gap-4 text-xs text-[#5a6580] mt-2">
                  <span>Anomalie max: <strong className="text-[#ff4444]">{analysis?.fractal?.maxAnomaly?.toFixed(3) || '—'}</strong></span>
                  <span>Rounds faibles: <strong className="text-[#ffaa00]">{analysis?.fractal?.anomalyRounds?.join(', ') || 'aucun'}</strong></span>
                  <span>Échelles faibles: <strong className="text-[#ffaa00]">{analysis?.fractal?.anomalyScales?.join(', ') || 'aucun'}</strong></span>
                </div>
              </CardContent>
            </Card>
            <Card className="bg-[#0c1220] border-[#1a2540]">
              <CardHeader className="pb-2"><CardTitle className="text-[#00ccff] text-xs tracking-[2px]">Top Anomalies Détectées</CardTitle></CardHeader>
              <CardContent>
                <div className="overflow-x-auto">
                  <table className="w-full text-xs">
                    <thead><tr className="text-[#5a6580] tracking-wider uppercase border-b border-[#1a2540]"><th className="p-2 text-left">Round</th><th className="p-2 text-left">Échelle</th><th className="p-2 text-left">Score</th></tr></thead>
                    <tbody>
                      {analysis?.fractal?.topAnomalies?.length ? analysis.fractal.topAnomalies.map((a: any, i: number) => (
                        <tr key={i} className="border-b border-[#0f1520]"><td className="p-2">{a.round}</td><td className="p-2">{a.scale}</td><td className={`p-2 font-bold ${a.score > 5 ? 'text-red-400' : a.score > 3 ? 'text-yellow-400' : ''}`}>{a.score.toFixed(3)}</td></tr>
                      )) : <tr><td colSpan={3} className="p-2 text-[#5a6580]">En attente d'initialisation...</td></tr>}
                    </tbody>
                  </table>
                </div>
              </CardContent>
            </Card>
          </div>
        )}

        {activeTab === 'inversion' && (
          <div className="space-y-4">
            {/* Found Key */}
            {found && privateKey && (
              <Card className="bg-gradient-to-br from-green-950 to-green-900 border-2 border-green-400 shadow-[0_0_30px_rgba(0,255,136,0.2)]">
                <CardContent className="p-6 text-center">
                  <h3 className="text-green-400 text-lg font-black tracking-[4px] mb-3">★★★ CLÉ PRIVÉE TROUVÉE ★★★</h3>
                  <div className="bg-[#060a12] rounded-md p-3 text-xl font-black text-green-400 font-mono break-all mb-2">{privateKey}</div>
                </CardContent>
              </Card>
            )}

            {/* Stats */}
            <Card className="bg-[#0c1220] border-[#1a2540] border-[#00ff88] shadow-[0_0_15px_rgba(0,255,136,0.05)]">
              <CardHeader className="pb-2"><CardTitle className="text-[#00ff88] text-xs tracking-[3px]">MODULE D'INVERSION — Recherche Backend</CardTitle></CardHeader>
              <CardContent>
                <div className="grid grid-cols-3 md:grid-cols-6 gap-2 mb-4">
                  {[
                    { label: 'Itérations', value: stats.iterations.toLocaleString() },
                    { label: 'Kangaroo Steps', value: stats.kangarooSteps.toLocaleString() },
                    { label: 'Incremental Steps', value: stats.incrementalSteps.toLocaleString() },
                    { label: 'Temps', value: stats.elapsed ? stats.elapsed.toFixed(1) + 's' : '—' },
                    { label: 'Clés/s', value: stats.keysPerSec },
                    { label: 'Stratégie', value: strategy },
                  ].map((item, i) => (
                    <div key={i} className="bg-[#060a12] border border-[#1a2540] rounded p-2 text-center">
                      <div className="text-[7px] text-[#5a6580] tracking-wider uppercase">{item.label}</div>
                      <div className="text-base font-black text-[#00ff88]">{item.value}</div>
                    </div>
                  ))}
                </div>
              </CardContent>
            </Card>

            {/* Log */}
            <Card className="bg-[#0c1220] border-[#1a2540]">
              <CardHeader className="pb-2"><CardTitle className="text-[#00ccff] text-xs tracking-[2px]">Journal d'Inversion</CardTitle></CardHeader>
              <CardContent>
                <ScrollArea className="h-64 w-full">
                  <div ref={logRef} className="space-y-0.5">
                    {logs.length === 0 ? (
                      <p className="text-[#5a6580] text-xs">En attente de connexion backend...</p>
                    ) : logs.map((log, i) => (
                      <p key={i} className={`text-[10px] border-b border-[#0f1520] py-0.5 ${logColor(log.type)}`}>
                        [{log.time}] {log.msg}
                      </p>
                    ))}
                  </div>
                </ScrollArea>
              </CardContent>
            </Card>
          </div>
        )}
      </div>
    </div>
  );
}
