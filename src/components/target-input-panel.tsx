'use client';

import React, { useState, useMemo, useCallback, useEffect } from "react";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Progress } from "@/components/ui/progress";
import DiscreteFractalPanel from "@/components/discrete-fractal-panel";
import {
  validatePublicKey,
  bytesToHex,
  hexToBytes,
} from "@/lib/secp256k1";
import type { Point } from "@/lib/secp256k1";
import { sha256Full, hashToHex } from "@/lib/sha256-engine";
import { pubkeyToSha256Block, generateRandomInput33, input33ToSha256Block } from "@/lib/bitcoin-pipeline";
import { computeFullDiscreteAnalysis } from "@/lib/discrete-fractal";
import type { FullDiscreteAnalysis } from "@/lib/discrete-fractal";
import { pubkeyToAddress, hash160 } from "@/lib/bitcoin-address";
import {
  Target,
  Activity,
  Hash,
  GitCompare,
  AlertTriangle,
  Key,
  FileCode,
  Bitcoin,
} from "lucide-react";

// --- Helpers ---

/** Hamming weight of a hex string */
function hammingWeightHex(hex: string): number {
  let weight = 0;
  for (let i = 0; i < hex.length; i++) {
    const nibble = parseInt(hex[i], 16);
    weight += [0, 1, 1, 2, 1, 2, 2, 3, 1, 2, 2, 3, 2, 3, 3, 4][nibble];
  }
  return weight;
}

/** Compute SHA-256 hash hex of a compressed pubkey */
function sha256OfCompressedPubkey(compressedHex: string): string {
  const bytes = hexToBytes(compressedHex);
  const hash = sha256Full(bytes);
  return hashToHex(hash);
}

/** Compute Hamming distance between two hex strings */
function hammingDistHex(hex1: string, hex2: string): number {
  if (hex1.length !== hex2.length) return 256;
  let dist = 0;
  for (let i = 0; i < hex1.length; i++) {
    const v1 = parseInt(hex1[i], 16);
    const v2 = parseInt(hex2[i], 16);
    const xor = v1 ^ v2;
    dist += [0, 1, 1, 2, 1, 2, 2, 3, 1, 2, 2, 3, 2, 3, 3, 4][xor];
  }
  return dist;
}

// --- Types ---

interface ComparisonMetrics {
  avgDimension: number;
  spectralFlatness: number;
  selfSimilarity: number;
  anomalyRounds: number;
  minDimension: number;
  minDimensionRound: number;
}

interface Finding {
  type: "anomaly" | "info" | "structure" | "random";
  section: string;
  message: string;
  details?: string;
}

// --- Section A: Target Input (NO PUZZLE — Manual Input Only) ---

function TargetInputSection({
  onTargetChange,
}: {
  onTargetChange: (data: {
    pubkeyHex: string | null;
    hashHex: string | null;
    address: string | null;
    point: Point | null;
  }) => void;
}) {
  const [pubkeyInput, setPubkeyInput] = useState("");
  const [hashInput, setHashInput] = useState("");
  const [addressInput, setAddressInput] = useState("");
  const [validationMsg, setValidationMsg] = useState<{ valid: boolean; msg: string } | null>(null);
  const [computedData, setComputedData] = useState<{
    sha256OfPubkey: string | null;
    derivedAddress: string | null;
    hash160Hex: string | null;
  }>({ sha256OfPubkey: null, derivedAddress: null, hash160Hex: null });

  const handleAnalyze = useCallback(() => {
    const pubkey = pubkeyInput.trim().replace(/\s/g, "");
    const hash = hashInput.trim().replace(/\s/g, "");
    const address = addressInput.trim();

    // Au moins un champ requis
    if (!pubkey && !hash && !address) {
      setValidationMsg({ valid: false, msg: "Entrez au moins une pubkey, un hash ou une adresse" });
      return;
    }

    // Valider le hash si fourni (64 hex chars = 256 bits)
    if (hash && !/^[0-9a-fA-F]{64}$/.test(hash)) {
      setValidationMsg({ valid: false, msg: "Hash invalide — 64 hex chars requis (256 bits)" });
      return;
    }

    // Valider la pubkey si fournie
    if (pubkey) {
      if (pubkey.length !== 66 && pubkey.length !== 130) {
        setValidationMsg({ valid: false, msg: "Pubkey invalide — 66 (compressée) ou 130 (décompressée) hex chars" });
        return;
      }
      const result = validatePublicKey(pubkey);
      if (!result.valid) {
        setValidationMsg({ valid: false, msg: `Pubkey invalide: ${result.error}` });
        return;
      }

      // Compute SHA-256 and address from pubkey
      try {
        const compressedHex = pubkey.length === 66 ? pubkey : pubkey; // both work
        const sha256Hex = sha256OfCompressedPubkey(compressedHex);
        const derivedAddr = pubkeyToAddress(compressedHex);
        const h160 = hash160(compressedHex);
        const h160Hex = Array.from(h160).map(b => b.toString(16).padStart(2, '0')).join('');

        setComputedData({ sha256OfPubkey: sha256Hex, derivedAddress: derivedAddr, hash160Hex: h160Hex });

        // Verify hash match if both provided
        if (hash && hash.toLowerCase() !== sha256Hex.toLowerCase()) {
          setValidationMsg({ valid: false, msg: "Le hash fourni ne correspond PAS à SHA-256(pubkey)" });
          return;
        }

        // Verify address match if both provided
        if (address && address !== derivedAddr) {
          setValidationMsg({ valid: false, msg: `L'adresse ne correspond PAS — attendue: ${derivedAddr.slice(0, 12)}...` });
          return;
        }

        setValidationMsg({ valid: true, msg: "Cible validée — Pubkey + SHA-256 + Adresse vérifiés" });
        onTargetChange({
          pubkeyHex: compressedHex,
          hashHex: hash || sha256Hex,
          address: address || derivedAddr,
          point: result.point || null,
        });
        return;
      } catch (e) {
        setValidationMsg({ valid: false, msg: `Erreur: ${(e as Error).message}` });
        return;
      }
    }

    // Only hash provided — direct hash mode
    if (hash) {
      setComputedData({ sha256OfPubkey: null, derivedAddress: null, hash160Hex: null });
      setValidationMsg({ valid: true, msg: "Mode hash direct — inversion sans pubkey" });
      onTargetChange({
        pubkeyHex: null,
        hashHex: hash,
        address: address || null,
        point: null,
      });
      return;
    }

    // Only address provided — limited mode
    setComputedData({ sha256OfPubkey: null, derivedAddress: null, hash160Hex: null });
    setValidationMsg({ valid: true, msg: "Adresse fournie — nécessite pubkey ou hash pour l'analyse fractale" });
    onTargetChange({
      pubkeyHex: null,
      hashHex: null,
      address: address,
      point: null,
    });
  }, [pubkeyInput, hashInput, addressInput, onTargetChange]);

  const handleReset = useCallback(() => {
    setPubkeyInput("");
    setHashInput("");
    setAddressInput("");
    setValidationMsg(null);
    setComputedData({ sha256OfPubkey: null, derivedAddress: null, hash160Hex: null });
    onTargetChange({ pubkeyHex: null, hashHex: null, address: null, point: null });
  }, [onTargetChange]);

  const currentPubkey = pubkeyInput.trim().replace(/\s/g, "") || null;
  const currentHash = hashInput.trim().replace(/\s/g, "") || computedData.sha256OfPubkey;

  return (
    <Card className="bg-zinc-900/60 border-zinc-800">
      <CardHeader className="pb-2 pt-3 px-4">
        <CardTitle className="text-xs font-semibold text-zinc-400 uppercase tracking-wider flex items-center gap-2">
          <Target className="h-3.5 w-3.5 text-orange-400" />
          CIBLE — Adresse / Pubkey / Hash
          <span className="text-[8px] text-zinc-600 font-mono normal-case ml-2">
            — Entrez manuellement votre cible
          </span>
        </CardTitle>
      </CardHeader>
      <CardContent className="px-4 pb-4 space-y-3">
        {/* Three input fields — no puzzle list */}
        <div className="grid grid-cols-1 gap-3">
          {/* Public Key Input */}
          <div>
            <label className="text-[9px] text-zinc-500 uppercase mb-1.5 flex items-center gap-1.5">
              <Key className="h-3 w-3 text-orange-400/70" />
              Public Key (compressée 66 / décompressée 130 hex)
            </label>
            <Input
              value={pubkeyInput}
              onChange={(e) => setPubkeyInput(e.target.value)}
              className="font-mono text-[10px] bg-zinc-950 border-zinc-700 text-orange-300 h-8 placeholder:text-zinc-700"
              placeholder="02... ou 03... ou 04..."
              spellCheck={false}
            />
          </div>

          {/* Hash Input */}
          <div>
            <label className="text-[9px] text-zinc-500 uppercase mb-1.5 flex items-center gap-1.5">
              <Hash className="h-3 w-3 text-cyan-400/70" />
              Hash SHA-256 (64 hex chars — cible directe)
            </label>
            <Input
              value={hashInput}
              onChange={(e) => setHashInput(e.target.value)}
              className="font-mono text-[10px] bg-zinc-950 border-zinc-700 text-cyan-300 h-8 placeholder:text-zinc-700"
              placeholder="a1b2c3... 64 hex chars"
              spellCheck={false}
            />
          </div>

          {/* Address Input */}
          <div>
            <label className="text-[9px] text-zinc-500 uppercase mb-1.5 flex items-center gap-1.5">
              <Bitcoin className="h-3 w-3 text-emerald-400/70" />
              Adresse Bitcoin (vérification croisée)
            </label>
            <Input
              value={addressInput}
              onChange={(e) => setAddressInput(e.target.value)}
              className="font-mono text-[10px] bg-zinc-950 border-zinc-700 text-emerald-300 h-8 placeholder:text-zinc-700"
              placeholder="1... ou 3... ou bc1..."
              spellCheck={false}
            />
          </div>
        </div>

        {/* Action buttons */}
        <div className="flex items-center gap-2">
          <Button
            size="sm"
            onClick={handleAnalyze}
            className="h-7 text-[10px] bg-orange-600 hover:bg-orange-500 text-white"
          >
            <Target className="h-3 w-3 mr-1" />
            ANALYSER & INIT INVERSION
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={handleReset}
            className="h-7 text-[10px] border-zinc-700 text-zinc-400"
          >
            RESET
          </Button>
        </div>

        {/* Validation message */}
        {validationMsg && (
          <Badge
            variant="outline"
            className={`text-[8px] ${
              validationMsg.valid ? "border-emerald-500/50 text-emerald-400" : "border-red-500/50 text-red-400"
            }`}
          >
            {validationMsg.valid ? "✓" : "✗"} {validationMsg.msg}
          </Badge>
        )}

        {/* Computed data display */}
        {computedData.sha256OfPubkey && (
          <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-3 space-y-1.5">
            <div className="flex items-start gap-2">
              <span className="text-[8px] text-zinc-600 uppercase w-14 shrink-0">Pubkey:</span>
              <span className="text-[9px] text-orange-400 font-mono break-all">{currentPubkey}</span>
            </div>
            <div className="flex items-start gap-2">
              <span className="text-[8px] text-zinc-600 uppercase w-14 shrink-0">SHA-256:</span>
              <span className="text-[9px] text-cyan-400 font-mono break-all">{computedData.sha256OfPubkey}</span>
            </div>
            {computedData.hash160Hex && (
              <div className="flex items-start gap-2">
                <span className="text-[8px] text-zinc-600 uppercase w-14 shrink-0">Hash160:</span>
                <span className="text-[9px] text-purple-400 font-mono break-all">{computedData.hash160Hex}</span>
              </div>
            )}
            {computedData.derivedAddress && (
              <div className="flex items-start gap-2">
                <span className="text-[8px] text-zinc-600 uppercase w-14 shrink-0">Adresse:</span>
                <span className="text-[9px] text-emerald-400 font-mono break-all">{computedData.derivedAddress}</span>
              </div>
            )}
            <div className="flex items-start gap-2">
              <span className="text-[8px] text-zinc-600 uppercase w-14 shrink-0">Poids:</span>
              <span className="text-[9px] text-zinc-400 font-mono">
                {hammingWeightHex(computedData.sha256OfPubkey)} / 256 bits ({((hammingWeightHex(computedData.sha256OfPubkey) / 256) * 100).toFixed(1)}%)
              </span>
            </div>
          </div>
        )}

        {currentHash && !computedData.sha256OfPubkey && (
          <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-3 space-y-1.5">
            <div className="flex items-start gap-2">
              <span className="text-[8px] text-zinc-600 uppercase w-14 shrink-0">Hash:</span>
              <span className="text-[9px] text-cyan-400 font-mono break-all">{currentHash}</span>
            </div>
            <div className="flex items-start gap-2">
              <span className="text-[8px] text-zinc-600 uppercase w-14 shrink-0">Poids:</span>
              <span className="text-[9px] text-zinc-400 font-mono">
                {hammingWeightHex(currentHash)} / 256 bits ({((hammingWeightHex(currentHash) / 256) * 100).toFixed(1)}%)
              </span>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// --- Section B: SHA-256 Fractal Analysis ---

function FractalAnalysisSection({
  targetPubkeyHex,
  targetHashHex,
  onAnalysisComplete,
}: {
  targetPubkeyHex: string | null;
  targetHashHex: string | null;
  onAnalysisComplete: (analysis: FullDiscreteAnalysis) => void;
}) {
  const [analysis, setAnalysis] = useState<FullDiscreteAnalysis | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [progress, setProgress] = useState(0);
  const [activeFractalRound, setActiveFractalRound] = useState(0);

  const canAnalyze = targetPubkeyHex || targetHashHex;

  const handleAnalyze = useCallback(() => {
    if (!targetPubkeyHex && !targetHashHex) return;
    setIsLoading(true);
    setProgress(0);
    setAnalysis(null);

    setTimeout(() => {
      try {
        let block: Uint8Array;
        if (targetPubkeyHex) {
          block = pubkeyToSha256Block(targetPubkeyHex);
        } else if (targetHashHex) {
          // Use hash bytes directly as a 64-byte SHA-256 block
          const hashBytes = hexToBytes(targetHashHex);
          block = new Uint8Array(64);
          block.set(hashBytes);
          // Add padding for 32-byte message
          block[32] = 0x80;
          block[63] = 0x00; // 256 bits = 0x100
          block[62] = 0x01;
        } else {
          return;
        }
        const result = computeFullDiscreteAnalysis(block, (pct) => {
          setProgress(pct);
        });
        setAnalysis(result);
        onAnalysisComplete(result);
      } catch (e) {
        console.error("Fractal analysis error:", e);
      }
      setIsLoading(false);
    }, 50);
  }, [targetPubkeyHex, targetHashHex, onAnalysisComplete]);

  const summary = useMemo(() => {
    if (!analysis) return null;
    const anomalyRounds = analysis.dimensionProfile.filter((dp) => dp.isAnomaly).length;
    const avgFlatness = analysis.walshSpectrum.reduce((s, ws) => s + ws.spectralFlatness, 0) / 64;
    const avgSelfSim = analysis.selfSimilarity.reduce((s, ss) => s + ss.selfSimilarityScore, 0) / 64;
    const avgMinDim = analysis.dimensionProfile.reduce((s, dp) => s + dp.minDimension, 0) / 64;
    const minDim = Math.min(...analysis.dimensionProfile.map(dp => dp.minDimension));
    const minDimRound = analysis.dimensionProfile.findIndex(dp => dp.minDimension === minDim);
    return { anomalyRounds, avgFlatness, avgSelfSim, avgMinDim, minDim, minDimRound };
  }, [analysis]);

  return (
    <Card className="bg-zinc-900/60 border-zinc-800">
      <CardHeader className="pb-2 pt-3 px-4">
        <CardTitle className="text-xs font-semibold text-zinc-400 uppercase tracking-wider flex items-center gap-2">
          <Activity className="h-3.5 w-3.5 text-emerald-400" />
          SHA-256 Fractal Analysis on Target
          <span className="text-[8px] text-zinc-600 font-mono normal-case ml-2">
            — Does SHA-256 behave differently on this target?
          </span>
        </CardTitle>
      </CardHeader>
      <CardContent className="px-4 pb-4 space-y-3">
        <div className="flex items-center gap-3 flex-wrap">
          <Button
            size="sm"
            onClick={handleAnalyze}
            disabled={isLoading || !canAnalyze}
            className="h-7 text-[10px] bg-emerald-600 hover:bg-emerald-500 text-white"
          >
            {isLoading ? (
              <span className="flex items-center gap-1">
                <svg className="animate-spin w-3 h-3" viewBox="0 0 24 24">
                  <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
                  <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                </svg>
                Analyzing... {progress}%
              </span>
            ) : (
              <>
                <Hash className="h-3 w-3 mr-1" />
                Analyze SHA-256(target)
              </>
            )}
          </Button>
          {isLoading && <Progress value={progress} className="h-1.5 flex-1 max-w-[200px]" />}
        </div>

        {/* Key metrics */}
        {summary && (
          <div className="grid grid-cols-2 sm:grid-cols-5 gap-2">
            <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-2 text-center">
              <div className="text-[8px] text-zinc-500 uppercase">Anomalies</div>
              <div className={`text-sm font-mono font-bold ${summary.anomalyRounds > 0 ? "text-red-400" : "text-emerald-400"}`}>
                {summary.anomalyRounds}/64
              </div>
            </div>
            <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-2 text-center">
              <div className="text-[8px] text-zinc-500 uppercase">Min Dimension</div>
              <div className={`text-sm font-mono font-bold ${summary.minDim < 200 ? "text-red-400" : summary.minDim < 240 ? "text-orange-400" : "text-emerald-400"}`}>
                {summary.minDim.toFixed(1)}
              </div>
              <div className="text-[7px] text-zinc-600">at R{summary.minDimRound}</div>
            </div>
            <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-2 text-center">
              <div className="text-[8px] text-zinc-500 uppercase">Avg Dimension</div>
              <div className="text-sm font-mono font-bold text-cyan-400">{summary.avgMinDim.toFixed(1)}</div>
            </div>
            <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-2 text-center">
              <div className="text-[8px] text-zinc-500 uppercase">Spectral Flat.</div>
              <div className={`text-sm font-mono font-bold ${summary.avgFlatness < 0.95 ? "text-orange-400" : "text-emerald-400"}`}>
                {summary.avgFlatness.toFixed(3)}
              </div>
            </div>
            <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-2 text-center">
              <div className="text-[8px] text-zinc-500 uppercase">Self-Similarity</div>
              <div className={`text-sm font-mono font-bold ${summary.avgSelfSim < 0.3 ? "text-red-400" : "text-emerald-400"}`}>
                {summary.avgSelfSim.toFixed(3)}
              </div>
            </div>
          </div>
        )}

        {/* Full fractal panel */}
        {analysis && (
          <DiscreteFractalPanel
            analysis={analysis}
            currentRound={activeFractalRound}
            onRoundChange={setActiveFractalRound}
          />
        )}

        {!analysis && !isLoading && canAnalyze && (
          <div className="bg-zinc-950/40 border border-zinc-800/50 rounded-lg p-6 text-center text-[10px] text-zinc-600">
            Click &quot;Analyze SHA-256(target)&quot; to compute the full discrete fractal analysis.
            <div className="text-[8px] text-zinc-700 font-mono mt-1">
              257 compression traces × 64 rounds = 16,448 state vectors analyzed
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// --- Section C: Comparison ---

function ComparisonSection({
  targetPubkeyHex,
  pubkeyAnalysis,
  onFindings,
}: {
  targetPubkeyHex: string | null;
  pubkeyAnalysis: FullDiscreteAnalysis | null;
  onFindings: (findings: Finding[]) => void;
}) {
  const [randomAnalysis, setRandomAnalysis] = useState<FullDiscreteAnalysis | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [progress, setProgress] = useState(0);
  const [randomInputHex, setRandomInputHex] = useState<string | null>(null);

  const handleCompare = useCallback(() => {
    if (!targetPubkeyHex) return;
    setIsLoading(true);
    setProgress(0);

    setTimeout(() => {
      try {
        const randomInput = generateRandomInput33();
        setRandomInputHex(bytesToHex(randomInput));
        const randomBlock = input33ToSha256Block(randomInput);
        const result = computeFullDiscreteAnalysis(randomBlock, (pct) => {
          setProgress(pct);
        });
        setRandomAnalysis(result);
      } catch (e) {
        console.error("Comparison analysis error:", e);
      }
      setIsLoading(false);
    }, 50);
  }, [targetPubkeyHex]);

  const comparison = useMemo(() => {
    if (!pubkeyAnalysis || !randomAnalysis) return null;

    const computeMetrics = (analysis: FullDiscreteAnalysis): ComparisonMetrics => {
      const anomalyRounds = analysis.dimensionProfile.filter((dp) => dp.isAnomaly).length;
      const avgFlatness = analysis.walshSpectrum.reduce((s, ws) => s + ws.spectralFlatness, 0) / 64;
      const avgSelfSim = analysis.selfSimilarity.reduce((s, ss) => s + ss.selfSimilarityScore, 0) / 64;
      const avgMinDim = analysis.dimensionProfile.reduce((s, dp) => s + dp.minDimension, 0) / 64;
      const minDim = Math.min(...analysis.dimensionProfile.map(dp => dp.minDimension));
      const minDimRound = analysis.dimensionProfile.findIndex(dp => dp.minDimension === minDim);
      return { avgDimension: avgMinDim, spectralFlatness: avgFlatness, selfSimilarity: avgSelfSim, anomalyRounds, minDimension: minDim, minDimensionRound: minDimRound };
    };

    const pub = computeMetrics(pubkeyAnalysis);
    const rnd = computeMetrics(randomAnalysis);

    const diff = {
      avgDimension: Math.abs(pub.avgDimension - rnd.avgDimension),
      spectralFlatness: Math.abs(pub.spectralFlatness - rnd.spectralFlatness),
      selfSimilarity: Math.abs(pub.selfSimilarity - rnd.selfSimilarity),
      anomalyRounds: Math.abs(pub.anomalyRounds - rnd.anomalyRounds),
    };

    const significant: { metric: string; pubkeyVal: number; randomVal: number; diff: number; pctDiff: number }[] = [];
    if (rnd.avgDimension > 0 && diff.avgDimension / rnd.avgDimension > 0.05) {
      significant.push({ metric: "Avg Dimension", pubkeyVal: pub.avgDimension, randomVal: rnd.avgDimension, diff: diff.avgDimension, pctDiff: diff.avgDimension / rnd.avgDimension * 100 });
    }
    if (rnd.spectralFlatness > 0 && diff.spectralFlatness / rnd.spectralFlatness > 0.05) {
      significant.push({ metric: "Spectral Flatness", pubkeyVal: pub.spectralFlatness, randomVal: rnd.spectralFlatness, diff: diff.spectralFlatness, pctDiff: diff.spectralFlatness / rnd.spectralFlatness * 100 });
    }
    if (rnd.selfSimilarity > 0 && diff.selfSimilarity / rnd.selfSimilarity > 0.05) {
      significant.push({ metric: "Self-Similarity", pubkeyVal: pub.selfSimilarity, randomVal: rnd.selfSimilarity, diff: diff.selfSimilarity, pctDiff: diff.selfSimilarity / rnd.selfSimilarity * 100 });
    }
    if (diff.anomalyRounds > 3) {
      significant.push({ metric: "Anomaly Rounds", pubkeyVal: pub.anomalyRounds, randomVal: rnd.anomalyRounds, diff: diff.anomalyRounds, pctDiff: diff.anomalyRounds });
    }

    const findings: Finding[] = [];
    if (significant.length > 0) {
      findings.push({
        type: "structure",
        section: "Comparison",
        message: `SHA-256(target) ≠ SHA-256(random) — ${significant.length} metric(s) differ by >5%`,
        details: significant.map(s => `${s.metric}: target=${s.pubkeyVal.toFixed(3)} vs random=${s.randomVal.toFixed(3)} (Δ${s.pctDiff.toFixed(1)}%)`).join("; "),
      });
    } else {
      findings.push({
        type: "random",
        section: "Comparison",
        message: "SHA-256 appears to treat target and random data equivalently — no difference >5%",
      });
    }

    if (pub.anomalyRounds > 0 && pub.anomalyRounds > rnd.anomalyRounds + 3) {
      findings.push({
        type: "anomaly",
        section: "Comparison",
        message: `Target has ${pub.anomalyRounds} anomaly rounds vs ${rnd.anomalyRounds} for random — possible structure`,
        details: `Min dimension: ${pub.minDimension.toFixed(1)} at R${pub.minDimensionRound} vs ${rnd.minDimension.toFixed(1)} at R${rnd.minDimensionRound}`,
      });
    }

    return { pub, rnd, diff, significant, findings };
  }, [pubkeyAnalysis, randomAnalysis]);

  useEffect(() => {
    if (comparison) {
      onFindings(comparison.findings);
    }
  }, [comparison, onFindings]);

  return (
    <Card className="bg-zinc-900/60 border-zinc-800">
      <CardHeader className="pb-2 pt-3 px-4">
        <CardTitle className="text-xs font-semibold text-zinc-400 uppercase tracking-wider flex items-center gap-2">
          <GitCompare className="h-3.5 w-3.5 text-cyan-400" />
          Target vs Random — Critical Comparison
          <span className="text-[8px] text-zinc-600 font-mono normal-case ml-2">
            — If SHA-256(target) ≠ SHA-256(random), we found structure
          </span>
        </CardTitle>
      </CardHeader>
      <CardContent className="px-4 pb-4 space-y-3">
        <div className="flex items-center gap-3 flex-wrap">
          <Button
            size="sm"
            onClick={handleCompare}
            disabled={isLoading || !targetPubkeyHex || !pubkeyAnalysis}
            className="h-7 text-[10px] bg-cyan-600 hover:bg-cyan-500 text-white"
          >
            {isLoading ? (
              <span className="flex items-center gap-1">
                <svg className="animate-spin w-3 h-3" viewBox="0 0 24 24">
                  <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
                  <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                </svg>
                Computing random... {progress}%
              </span>
            ) : (
              <>
                <GitCompare className="h-3 w-3 mr-1" />
                Compare with Random Input
              </>
            )}
          </Button>
          {!pubkeyAnalysis && targetPubkeyHex && (
            <Badge variant="outline" className="text-[8px] border-orange-500/50 text-orange-400">
              Run fractal analysis first
            </Badge>
          )}
        </div>

        {randomInputHex && (
          <div className="text-[8px] text-zinc-600 font-mono">
            Random input: {randomInputHex.slice(0, 20)}...{randomInputHex.slice(-8)}
          </div>
        )}

        {comparison && (
          <div className="space-y-4">
            {comparison.significant.length > 0 ? (
              <div className="bg-red-950/30 border border-red-500/30 rounded-lg p-3">
                <div className="flex items-center gap-2 mb-1">
                  <AlertTriangle className="h-3.5 w-3.5 text-red-400" />
                  <span className="text-[10px] text-red-400 font-semibold uppercase">Structural Difference Detected</span>
                </div>
                <div className="space-y-1">
                  {comparison.significant.map((s, i) => (
                    <div key={i} className="text-[9px] font-mono">
                      <span className="text-red-300">{s.metric}:</span>{" "}
                      <span className="text-orange-400">target={s.pubkeyVal.toFixed(3)}</span>{" "}
                      <span className="text-zinc-500">vs</span>{" "}
                      <span className="text-cyan-400">random={s.randomVal.toFixed(3)}</span>{" "}
                      <span className="text-red-400">Δ{s.pctDiff.toFixed(1)}%</span>
                    </div>
                  ))}
                </div>
              </div>
            ) : (
              <div className="bg-emerald-950/20 border border-emerald-500/20 rounded-lg p-3">
                <div className="text-[10px] text-emerald-400 font-semibold">
                  No significant difference (&gt;5%) between SHA-256(target) and SHA-256(random)
                </div>
                <div className="text-[8px] text-zinc-500 mt-1">
                  SHA-256 appears to treat this input as random — no exploitable structure found in comparison
                </div>
              </div>
            )}

            <div className="grid grid-cols-2 sm:grid-cols-4 gap-2">
              {[
                { label: "Avg Dimension", pub: comparison.pub.avgDimension, rnd: comparison.rnd.avgDimension, format: (v: number) => v.toFixed(1) },
                { label: "Spectral Flat.", pub: comparison.pub.spectralFlatness, rnd: comparison.rnd.spectralFlatness, format: (v: number) => v.toFixed(4) },
                { label: "Self-Similarity", pub: comparison.pub.selfSimilarity, rnd: comparison.rnd.selfSimilarity, format: (v: number) => v.toFixed(4) },
                { label: "Anomaly Rounds", pub: comparison.pub.anomalyRounds, rnd: comparison.rnd.anomalyRounds, format: (v: number) => String(v) },
              ].map(({ label, pub, rnd, format }) => {
                const pctDiff = rnd > 0 ? Math.abs(pub - rnd) / rnd * 100 : 0;
                const isSignificant = pctDiff > 5;
                return (
                  <div key={label} className={`rounded-lg p-2 border ${isSignificant ? "border-red-500/30 bg-red-950/20" : "border-zinc-800 bg-zinc-950/60"}`}>
                    <div className="text-[7px] text-zinc-500 uppercase">{label}</div>
                    <div className="flex items-center gap-1.5 mt-1">
                      <span className="text-[10px] font-mono font-bold text-orange-400">{format(pub)}</span>
                      <span className="text-[7px] text-zinc-600">vs</span>
                      <span className="text-[10px] font-mono text-cyan-400">{format(rnd)}</span>
                    </div>
                    {isSignificant && (
                      <div className="text-[8px] text-red-400 font-mono mt-0.5">Δ{pctDiff.toFixed(1)}%</div>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {!randomAnalysis && !isLoading && pubkeyAnalysis && (
          <div className="bg-zinc-950/40 border border-zinc-800/50 rounded-lg p-4 text-center text-[10px] text-zinc-600">
            Click &quot;Compare with Random Input&quot; to generate a random 33-byte input and compare its SHA-256 fractal analysis against the target.
            <div className="text-[8px] text-cyan-500/50 font-mono mt-1">
              Key test: if SHA-256(target) ≠ SHA-256(random) in fractal structure → FOUND SOMETHING
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// --- Main Panel: Target Input (replaces Puzzle Analyzer) ---

export default function TargetInputPanel() {
  const [targetData, setTargetData] = useState<{
    pubkeyHex: string | null;
    hashHex: string | null;
    address: string | null;
    point: Point | null;
  }>({ pubkeyHex: null, hashHex: null, address: null, point: null });

  const [pubkeyAnalysis, setPubkeyAnalysis] = useState<FullDiscreteAnalysis | null>(null);
  const [findings, setFindings] = useState<Finding[]>([]);

  const handleTargetChange = useCallback((data: {
    pubkeyHex: string | null;
    hashHex: string | null;
    address: string | null;
    point: Point | null;
  }) => {
    setTargetData(data);
    // Reset analysis when target changes
    if (data.pubkeyHex !== targetData.pubkeyHex || data.hashHex !== targetData.hashHex) {
      setPubkeyAnalysis(null);
      setFindings([]);
    }
  }, [targetData]);

  const handleAnalysisComplete = useCallback((analysis: FullDiscreteAnalysis) => {
    setPubkeyAnalysis(analysis);
  }, []);

  const handleFindings = useCallback((newFindings: Finding[]) => {
    setFindings(newFindings);
  }, []);

  return (
    <div className="space-y-4">
      {/* Section A: Target Input — manual fields only, NO PUZZLE */}
      <TargetInputSection onTargetChange={handleTargetChange} />

      {/* Section B: Fractal Analysis */}
      <FractalAnalysisSection
        targetPubkeyHex={targetData.pubkeyHex}
        targetHashHex={targetData.hashHex}
        onAnalysisComplete={handleAnalysisComplete}
      />

      {/* Section C: Comparison */}
      <ComparisonSection
        targetPubkeyHex={targetData.pubkeyHex}
        pubkeyAnalysis={pubkeyAnalysis}
        onFindings={handleFindings}
      />

      {/* Findings Summary */}
      {findings.length > 0 && (
        <Card className="bg-zinc-900/60 border-zinc-800">
          <CardHeader className="pb-2 pt-3 px-4">
            <CardTitle className="text-xs font-semibold text-zinc-400 uppercase tracking-wider flex items-center gap-2">
              <FileCode className="h-3.5 w-3.5 text-yellow-400" />
              Findings Summary
              <Badge variant="outline" className="text-[8px] border-zinc-700 text-zinc-500 ml-auto">
                {findings.length} finding(s)
              </Badge>
            </CardTitle>
          </CardHeader>
          <CardContent className="px-4 pb-4 space-y-2">
            {findings.map((f, i) => (
              <div
                key={i}
                className={`rounded-lg p-2.5 border text-[9px] ${
                  f.type === "anomaly"
                    ? "border-red-500/30 bg-red-950/20"
                    : f.type === "structure"
                    ? "border-orange-500/30 bg-orange-950/20"
                    : f.type === "info"
                    ? "border-cyan-500/30 bg-cyan-950/20"
                    : "border-zinc-800 bg-zinc-950/40"
                }`}
              >
                <div className="font-semibold">
                  <span className={`${
                    f.type === "anomaly" ? "text-red-400" :
                    f.type === "structure" ? "text-orange-400" :
                    f.type === "info" ? "text-cyan-400" : "text-zinc-400"
                  }`}>
                    [{f.type.toUpperCase()}]
                  </span>{" "}
                  {f.message}
                </div>
                {f.details && (
                  <div className="text-zinc-500 font-mono mt-1 text-[8px]">{f.details}</div>
                )}
              </div>
            ))}
          </CardContent>
        </Card>
      )}
    </div>
  );
}
