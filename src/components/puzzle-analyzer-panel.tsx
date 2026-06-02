'use client';

import React, { useState, useMemo, useCallback, useRef, useEffect } from "react";
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
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  LineChart,
  Line,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ReferenceLine,
  ResponsiveContainer,
  BarChart,
  Bar,
  Cell,
  Legend,
  AreaChart,
  Area,
} from "recharts";
import DiscreteFractalPanel from "@/components/discrete-fractal-panel";
import {
  PUZZLES,
  getPuzzleByNumber,
  getUnsolvedPuzzles,
  getPuzzlesWithPublicKey,
  formatRange,
  getRangeSize,
} from "@/lib/puzzle-db";
import type { BitcoinPuzzle } from "@/lib/puzzle-db";
import {
  validatePublicKey,
  scalarMultiply,
  pointAdd,
  getGenerator,
  bytesToHex,
  hexToBytes,
  N as CURVE_N,
} from "@/lib/secp256k1";
import type { Point } from "@/lib/secp256k1";
import { sha256Full, hashToHex, compressBlock, flipBit } from "@/lib/sha256-engine";
import { pubkeyToSha256Block, input33ToSha256Block, generateRandomInput33 } from "@/lib/bitcoin-pipeline";
import { computeFullDiscreteAnalysis } from "@/lib/discrete-fractal";
import type { FullDiscreteAnalysis } from "@/lib/discrete-fractal";
import {
  Copy,
  Check,
  AlertTriangle,
  Search,
  Target,
  Activity,
  Hash,
  Download,
  GitCompare,
  ScanLine,
  FileText,
  Zap,
} from "lucide-react";

// --- Helpers ---

/** Convert an EC Point to compressed pubkey hex string */
function pointToCompressedHex(point: Point): string {
  const prefix = (point.y & 1n) === 0n ? "02" : "03";
  const xHex = point.x.toString(16).padStart(64, "0");
  return prefix + xHex;
}

/** Population count for a 32-bit unsigned integer */
function popcount32(x: number): number {
  x = x >>> 0;
  x = x - ((x >>> 1) & 0x55555555);
  x = (x & 0x33333333) + ((x >>> 2) & 0x33333333);
  x = (x + (x >>> 4)) & 0x0f0f0f0f;
  x = x + (x >>> 8);
  x = x + (x >>> 16);
  return x & 0x7f;
}

/** Compute Hamming distance between two state vectors (8 × 32-bit words = 256 bits) */
function hammingDistWords(v1: number[], v2: number[]): number {
  let dist = 0;
  for (let i = 0; i < 8; i++) {
    dist += popcount32((v1[i] ^ v2[i]) >>> 0);
  }
  return dist;
}

/** Quick dimension score: simplified fractal dimension estimate using 16 bit-flips */
function quickDimensionScore(block: Uint8Array): number {
  const baseTrace = compressBlock(block);
  const numFlips = 16;
  const finalDistances: number[] = [];

  for (let i = 0; i < numFlips; i++) {
    const flipBitIdx = i * 16; // spread out flips
    const modified = flipBit(block, flipBitIdx);
    const modTrace = compressBlock(modified);

    const baseFinal = baseTrace.rounds[63];
    const modFinal = modTrace.rounds[63];
    const dist = hammingDistWords(
      [baseFinal.a, baseFinal.b, baseFinal.c, baseFinal.d, baseFinal.e, baseFinal.f, baseFinal.g, baseFinal.h],
      [modFinal.a, modFinal.b, modFinal.c, modFinal.d, modFinal.e, modFinal.f, modFinal.g, modFinal.h]
    );
    finalDistances.push(dist);
  }

  const avgDist = finalDistances.reduce((a, b) => a + b, 0) / finalDistances.length;
  return avgDist / 128; // 1.0 = random-like
}

/** Quick diffusion profile: track Hamming distance growth across rounds */
function quickDiffusionProfile(block: Uint8Array): number[] {
  const baseTrace = compressBlock(block);
  const numFlips = 8;
  const roundDistances = new Array(64).fill(0);

  for (let i = 0; i < numFlips; i++) {
    const flipBitIdx = i * 32;
    const modified = flipBit(block, flipBitIdx);
    const modTrace = compressBlock(modified);

    for (let r = 0; r < 64; r++) {
      const baseR = baseTrace.rounds[r];
      const modR = modTrace.rounds[r];
      roundDistances[r] += hammingDistWords(
        [baseR.a, baseR.b, baseR.c, baseR.d, baseR.e, baseR.f, baseR.g, baseR.h],
        [modR.a, modR.b, modR.c, modR.d, modR.e, modR.f, modR.g, modR.h]
      );
    }
  }

  return roundDistances.map(d => d / numFlips);
}

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

// --- Types ---

interface ComparisonMetrics {
  avgDimension: number;
  spectralFlatness: number;
  selfSimilarity: number;
  anomalyRounds: number;
  minDimension: number;
  minDimensionRound: number;
}

interface NeighborResult {
  offset: number;
  compressedHex: string;
  sha256Hex: string;
  hammingWeight: number;
  dimScore: number;
  hammingDistFromTarget: number;
  diffusionProfile: number[];
}

interface Finding {
  type: "anomaly" | "info" | "structure" | "random";
  section: string;
  message: string;
  details?: string;
}

// --- Section A: Target Input ---

type PuzzleFilter = "all" | "unsolved" | "with_pubkey";

function TargetInputSection({
  onTargetChange,
}: {
  onTargetChange: (pubkeyHex: string | null, point: Point | null, puzzle: BitcoinPuzzle | null) => void;
}) {
  const [filter, setFilter] = useState<PuzzleFilter>("with_pubkey");
  const [searchTerm, setSearchTerm] = useState("");
  const [selectedPuzzle, setSelectedPuzzle] = useState<BitcoinPuzzle | null>(null);
  const [manualPubkey, setManualPubkey] = useState("");
  const [validationMsg, setValidationMsg] = useState<{ valid: boolean; msg: string } | null>(null);

  const filteredPuzzles = useMemo(() => {
    let list = PUZZLES;
    if (filter === "unsolved") list = getUnsolvedPuzzles();
    if (filter === "with_pubkey") list = getPuzzlesWithPublicKey();
    if (searchTerm) {
      const term = searchTerm.toLowerCase();
      list = list.filter(
        (p) =>
          p.number.toString().includes(term) ||
          p.address?.toLowerCase().includes(term) ||
          p.publicKeyCompressed?.toLowerCase().includes(term)
      );
    }
    return list;
  }, [filter, searchTerm]);

  const handleSelectPuzzle = useCallback((puzzle: BitcoinPuzzle) => {
    setSelectedPuzzle(puzzle);
    setValidationMsg(null);
    if (puzzle.publicKeyCompressed) {
      const result = validatePublicKey(puzzle.publicKeyCompressed);
      if (result.valid && result.point) {
        setValidationMsg({ valid: true, msg: `Valid secp256k1 point — Puzzle #${puzzle.number} pubkey` });
        onTargetChange(puzzle.publicKeyCompressed, result.point, puzzle);
      }
    } else {
      onTargetChange(null, null, puzzle);
    }
  }, [onTargetChange]);

  const handleValidate = useCallback(() => {
    if (!manualPubkey) {
      setValidationMsg({ valid: false, msg: "Enter a compressed public key (66 hex chars)" });
      return;
    }
    const result = validatePublicKey(manualPubkey);
    if (result.valid) {
      setValidationMsg({ valid: true, msg: "Valid secp256k1 public key" });
      setSelectedPuzzle(null);
      onTargetChange(manualPubkey, result.point || null, null);
    } else {
      setValidationMsg({ valid: false, msg: result.error || "Invalid public key" });
      onTargetChange(null, null, null);
    }
  }, [manualPubkey, onTargetChange]);

  // Compute SHA-256 of current pubkey for display
  const sha256Display = useMemo(() => {
    const pubkey = selectedPuzzle?.publicKeyCompressed || (validationMsg?.valid ? manualPubkey : null);
    if (!pubkey) return null;
    try {
      return sha256OfCompressedPubkey(pubkey);
    } catch {
      return null;
    }
  }, [selectedPuzzle, manualPubkey, validationMsg]);

  const currentPubkey = selectedPuzzle?.publicKeyCompressed || (validationMsg?.valid ? manualPubkey : null);

  return (
    <Card className="bg-zinc-900/60 border-zinc-800">
      <CardHeader className="pb-2 pt-3 px-4">
        <CardTitle className="text-xs font-semibold text-zinc-400 uppercase tracking-wider flex items-center gap-2">
          <Target className="h-3.5 w-3.5 text-orange-400" />
          Target Input
          <Badge variant="outline" className="text-[8px] border-zinc-700 text-zinc-500 ml-auto">
            {getPuzzlesWithPublicKey().length} puzzles with known pubkeys
          </Badge>
        </CardTitle>
      </CardHeader>
      <CardContent className="px-4 pb-4 space-y-3">
        {/* Puzzle selector */}
        <div>
          <label className="text-[9px] text-zinc-500 uppercase mb-1.5 block">Select Known Puzzle</label>
          <div className="flex items-center gap-2 flex-wrap">
            {(["all", "unsolved", "with_pubkey"] as PuzzleFilter[]).map((f) => (
              <Button
                key={f}
                size="sm"
                variant={filter === f ? "default" : "outline"}
                onClick={() => setFilter(f)}
                className={`h-6 text-[9px] ${
                  filter === f
                    ? f === "with_pubkey"
                      ? "bg-orange-600 hover:bg-orange-500 text-white"
                      : f === "unsolved"
                      ? "bg-red-600 hover:bg-red-500 text-white"
                      : "bg-zinc-600 hover:bg-zinc-500 text-white"
                    : "border-zinc-700 text-zinc-400"
                }`}
              >
                {f === "all" ? "All" : f === "unsolved" ? "Unsolved" : "With Pubkey"}
              </Button>
            ))}
            <Input
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              placeholder="Search #, address, pubkey..."
              className="font-mono text-[9px] bg-zinc-950 border-zinc-700 text-zinc-300 h-6 flex-1 min-w-[140px]"
            />
          </div>
          <div className="max-h-40 overflow-y-auto rounded-lg border border-zinc-800 bg-zinc-950/40 mt-2">
            <div className="sticky top-0 bg-zinc-900/95 backdrop-blur-sm border-b border-zinc-800 grid grid-cols-[36px_1fr_70px_50px] gap-1 px-2 py-1 text-[7px] text-zinc-500 uppercase font-semibold z-10">
              <span>#</span>
              <span>Pubkey / Address</span>
              <span>Range</span>
              <span>BTC</span>
            </div>
            {filteredPuzzles.map((puzzle) => (
              <div
                key={puzzle.number}
                className={`grid grid-cols-[36px_1fr_70px_50px] gap-1 px-2 py-1 border-b border-zinc-800/50 cursor-pointer hover:bg-zinc-800/40 transition-colors text-[8px] font-mono ${
                  selectedPuzzle?.number === puzzle.number ? "bg-orange-950/20" : ""
                }`}
                onClick={() => handleSelectPuzzle(puzzle)}
              >
                <span className="text-zinc-300 font-bold">{puzzle.number}</span>
                <span className={`truncate ${puzzle.publicKeyCompressed ? "text-orange-400" : "text-zinc-600"}`}>
                  {puzzle.publicKeyCompressed
                    ? puzzle.publicKeyCompressed.slice(0, 16) + "..."
                    : puzzle.address
                    ? puzzle.address.slice(0, 16) + "..."
                    : "—"}
                </span>
                <span className="text-zinc-600">{formatRange(puzzle.rangeStart, puzzle.rangeEnd)}</span>
                <span className="text-zinc-500">{puzzle.balance?.toFixed(1) || "—"}</span>
              </div>
            ))}
          </div>
        </div>

        {/* Manual pubkey input */}
        <div>
          <label className="text-[9px] text-zinc-500 uppercase mb-1 block">Manual Public Key (compressed, 66 hex chars)</label>
          <div className="flex gap-2">
            <Input
              value={manualPubkey}
              onChange={(e) => setManualPubkey(e.target.value)}
              className="font-mono text-[10px] bg-zinc-950 border-zinc-700 text-zinc-300 h-7 flex-1"
              placeholder="02... or 03..."
            />
            <Button
              size="sm"
              onClick={handleValidate}
              className="h-7 text-[10px] bg-orange-600 hover:bg-orange-500 text-white shrink-0"
            >
              Validate
            </Button>
          </div>
        </div>

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

        {/* Current target display */}
        {currentPubkey && (
          <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-3 space-y-1.5">
            <div className="flex items-start gap-2">
              <span className="text-[8px] text-zinc-600 uppercase w-14 shrink-0">Pubkey:</span>
              <span className="text-[9px] text-orange-400 font-mono break-all">{currentPubkey}</span>
            </div>
            {sha256Display && (
              <div className="flex items-start gap-2">
                <span className="text-[8px] text-zinc-600 uppercase w-14 shrink-0">SHA-256:</span>
                <span className="text-[9px] text-cyan-400 font-mono break-all">{sha256Display}</span>
              </div>
            )}
            {selectedPuzzle && (
              <div className="flex items-start gap-2">
                <span className="text-[8px] text-zinc-600 uppercase w-14 shrink-0">Range:</span>
                <span className="text-[9px] text-zinc-400 font-mono">
                  {formatRange(selectedPuzzle.rangeStart, selectedPuzzle.rangeEnd)} ({selectedPuzzle.number} bits)
                </span>
              </div>
            )}
            {sha256Display && (
              <div className="flex items-start gap-2">
                <span className="text-[8px] text-zinc-600 uppercase w-14 shrink-0">Weight:</span>
                <span className="text-[9px] text-zinc-400 font-mono">
                  {hammingWeightHex(sha256Display)} / 256 bits ({((hammingWeightHex(sha256Display) / 256) * 100).toFixed(1)}%)
                </span>
              </div>
            )}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// --- Section B: SHA-256 Fractal Analysis ---

function FractalAnalysisSection({
  targetPubkeyHex,
  onAnalysisComplete,
}: {
  targetPubkeyHex: string | null;
  onAnalysisComplete: (analysis: FullDiscreteAnalysis) => void;
}) {
  const [analysis, setAnalysis] = useState<FullDiscreteAnalysis | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [progress, setProgress] = useState(0);
  const [activeFractalRound, setActiveFractalRound] = useState(0);

  const handleAnalyze = useCallback(() => {
    if (!targetPubkeyHex) return;
    setIsLoading(true);
    setProgress(0);
    setAnalysis(null);

    setTimeout(() => {
      try {
        const pubkeyBlock = pubkeyToSha256Block(targetPubkeyHex);
        const result = computeFullDiscreteAnalysis(pubkeyBlock, (pct) => {
          setProgress(pct);
        });
        setAnalysis(result);
        onAnalysisComplete(result);
      } catch (e) {
        console.error("Fractal analysis error:", e);
      }
      setIsLoading(false);
    }, 50);
  }, [targetPubkeyHex, onAnalysisComplete]);

  // Compute summary
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
            — Does SHA-256 behave differently on this pubkey?
          </span>
        </CardTitle>
      </CardHeader>
      <CardContent className="px-4 pb-4 space-y-3">
        <div className="flex items-center gap-3 flex-wrap">
          <Button
            size="sm"
            onClick={handleAnalyze}
            disabled={isLoading || !targetPubkeyHex}
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
                Analyze SHA-256(target_pubkey)
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

        {!analysis && !isLoading && targetPubkeyHex && (
          <div className="bg-zinc-950/40 border border-zinc-800/50 rounded-lg p-6 text-center text-[10px] text-zinc-600">
            Click &quot;Analyze SHA-256(target_pubkey)&quot; to compute the full discrete fractal analysis.
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
        // Generate random 33-byte input
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

  // Compute comparison metrics
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

    // Compute differences
    const diff = {
      avgDimension: Math.abs(pub.avgDimension - rnd.avgDimension),
      spectralFlatness: Math.abs(pub.spectralFlatness - rnd.spectralFlatness),
      selfSimilarity: Math.abs(pub.selfSimilarity - rnd.selfSimilarity),
      anomalyRounds: Math.abs(pub.anomalyRounds - rnd.anomalyRounds),
    };

    // Detect significant differences (>5%)
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

    // Build findings
    const findings: Finding[] = [];
    if (significant.length > 0) {
      findings.push({
        type: "structure",
        section: "Comparison",
        message: `SHA-256(pubkey) ≠ SHA-256(random) — ${significant.length} metric(s) differ by >5%`,
        details: significant.map(s => `${s.metric}: pubkey=${s.pubkeyVal.toFixed(3)} vs random=${s.randomVal.toFixed(3)} (Δ${s.pctDiff.toFixed(1)}%)`).join("; "),
      });
    } else {
      findings.push({
        type: "random",
        section: "Comparison",
        message: "SHA-256 appears to treat pubkey and random data equivalently — no difference >5%",
      });
    }

    if (pub.anomalyRounds > 0 && pub.anomalyRounds > rnd.anomalyRounds + 3) {
      findings.push({
        type: "anomaly",
        section: "Comparison",
        message: `⚠ Pubkey has ${pub.anomalyRounds} anomaly rounds vs ${rnd.anomalyRounds} for random — possible structure`,
        details: `Min dimension: ${pub.minDimension.toFixed(1)} at R${pub.minDimensionRound} vs ${rnd.minDimension.toFixed(1)} at R${rnd.minDimensionRound}`,
      });
    }

    return { pub, rnd, diff, significant, findings };
  }, [pubkeyAnalysis, randomAnalysis]);

  // Report findings to parent
  useEffect(() => {
    if (comparison) {
      onFindings(comparison.findings);
    }
  }, [comparison, onFindings]);

  // Dimension profile overlay data
  const overlayData = useMemo(() => {
    if (!pubkeyAnalysis || !randomAnalysis) return [];
    const scales = pubkeyAnalysis.boxCounting[0]?.scales || [];
    return scales.slice(0, -1).map((scale, si) => {
      const pubAvgDim = pubkeyAnalysis.boxCounting.reduce((s, bc) => s + (bc.dimensionEstimates[si] || 256), 0) / 64;
      const rndAvgDim = randomAnalysis.boxCounting.reduce((s, bc) => s + (bc.dimensionEstimates[si] || 256), 0) / 64;
      return {
        scale: Math.log2(scale),
        pubkeyDim: pubAvgDim,
        randomDim: rndAvgDim,
      };
    });
  }, [pubkeyAnalysis, randomAnalysis]);

  // Per-round min dimension overlay
  const roundOverlayData = useMemo(() => {
    if (!pubkeyAnalysis || !randomAnalysis) return [];
    return pubkeyAnalysis.dimensionProfile.map((dp, i) => ({
      round: dp.round,
      pubkeyDim: dp.minDimension,
      randomDim: randomAnalysis.dimensionProfile[i]?.minDimension ?? 256,
    }));
  }, [pubkeyAnalysis, randomAnalysis]);

  // Bar comparison data
  const barData = useMemo(() => {
    if (!comparison) return [];
    return [
      { metric: "Avg Dim", pubkey: comparison.pub.avgDimension, random: comparison.rnd.avgDimension, isSignificant: comparison.diff.avgDimension / (comparison.rnd.avgDimension || 1) > 0.05 },
      { metric: "Spec. Flat.", pubkey: comparison.pub.spectralFlatness * 256, random: comparison.rnd.spectralFlatness * 256, isSignificant: comparison.diff.spectralFlatness / (comparison.rnd.spectralFlatness || 1) > 0.05 },
      { metric: "Self-Sim", pubkey: comparison.pub.selfSimilarity * 256, random: comparison.rnd.selfSimilarity * 256, isSignificant: comparison.diff.selfSimilarity / (comparison.rnd.selfSimilarity || 1) > 0.05 },
      { metric: "Anomalies", pubkey: comparison.pub.anomalyRounds, random: comparison.rnd.anomalyRounds, isSignificant: comparison.diff.anomalyRounds > 3 },
    ];
  }, [comparison]);

  return (
    <Card className="bg-zinc-900/60 border-zinc-800">
      <CardHeader className="pb-2 pt-3 px-4">
        <CardTitle className="text-xs font-semibold text-zinc-400 uppercase tracking-wider flex items-center gap-2">
          <GitCompare className="h-3.5 w-3.5 text-cyan-400" />
          Pubkey vs Random — Critical Comparison
          <span className="text-[8px] text-zinc-600 font-mono normal-case ml-2">
            — If SHA-256(pubkey) ≠ SHA-256(random), we found structure
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
              Run Section B analysis first
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
            {/* Significant differences alert */}
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
                      <span className="text-orange-400">pubkey={s.pubkeyVal.toFixed(3)}</span>{" "}
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
                  ✓ No significant difference (&gt;5%) between SHA-256(pubkey) and SHA-256(random)
                </div>
                <div className="text-[8px] text-zinc-500 mt-1">
                  SHA-256 appears to treat this pubkey as random input — no exploitable structure found in comparison
                </div>
              </div>
            )}

            {/* Metrics comparison cards */}
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
                      <div className="text-[8px] text-red-400 font-mono mt-0.5">Δ{pctDiff.toFixed(1)}% ⚠</div>
                    )}
                  </div>
                );
              })}
            </div>

            {/* Dimension profile overlay chart */}
            <div>
              <div className="text-[9px] text-zinc-500 uppercase mb-1">Dimension Profile: Pubkey vs Random (averaged over 64 rounds)</div>
              <div className="h-52">
                <ResponsiveContainer width="100%" height="100%">
                  <LineChart data={overlayData}>
                    <CartesianGrid strokeDasharray="3 3" stroke="#27272a" />
                    <XAxis dataKey="scale" tick={{ fill: "#71717a", fontSize: 9 }} label={{ value: "log₂(scale)", position: "insideBottom", offset: -2, style: { fill: "#71717a", fontSize: 8 } }} />
                    <YAxis domain={[0, 300]} tick={{ fill: "#71717a", fontSize: 9 }} label={{ value: "Dimension", angle: -90, position: "insideLeft", style: { fill: "#71717a", fontSize: 8 } }} />
                    <Tooltip contentStyle={{ backgroundColor: "#18181b", border: "1px solid #3f3f46", borderRadius: 8, fontSize: 10 }} />
                    <ReferenceLine y={256} stroke="#10b981" strokeDasharray="5 5" strokeOpacity={0.5} />
                    <ReferenceLine y={200} stroke="#f97316" strokeDasharray="5 5" strokeOpacity={0.5} />
                    <Line type="monotone" dataKey="pubkeyDim" stroke="#f97316" strokeWidth={2} name="Pubkey" dot={false} />
                    <Line type="monotone" dataKey="randomDim" stroke="#06b6d4" strokeWidth={2} name="Random" dot={false} strokeDasharray="6 3" />
                    <Legend wrapperStyle={{ fontSize: 9 }} />
                  </LineChart>
                </ResponsiveContainer>
              </div>
            </div>

            {/* Per-round min dimension overlay */}
            <div>
              <div className="text-[9px] text-zinc-500 uppercase mb-1">Min Dimension by Round: Pubkey vs Random</div>
              <div className="h-44">
                <ResponsiveContainer width="100%" height="100%">
                  <AreaChart data={roundOverlayData}>
                    <CartesianGrid strokeDasharray="3 3" stroke="#27272a" />
                    <XAxis dataKey="round" tick={{ fill: "#71717a", fontSize: 9 }} />
                    <YAxis domain={[0, 300]} tick={{ fill: "#71717a", fontSize: 9 }} />
                    <Tooltip contentStyle={{ backgroundColor: "#18181b", border: "1px solid #3f3f46", borderRadius: 8, fontSize: 10 }} />
                    <ReferenceLine y={200} stroke="#f97316" strokeDasharray="3 3" strokeOpacity={0.3} />
                    <Area type="monotone" dataKey="pubkeyDim" stroke="#f97316" fill="#f97316" fillOpacity={0.1} strokeWidth={1.5} name="Pubkey" />
                    <Area type="monotone" dataKey="randomDim" stroke="#06b6d4" fill="#06b6d4" fillOpacity={0.05} strokeWidth={1.5} name="Random" strokeDasharray="6 3" />
                    <Legend wrapperStyle={{ fontSize: 9 }} />
                  </AreaChart>
                </ResponsiveContainer>
              </div>
            </div>
          </div>
        )}

        {!randomAnalysis && !isLoading && pubkeyAnalysis && (
          <div className="bg-zinc-950/40 border border-zinc-800/50 rounded-lg p-4 text-center text-[10px] text-zinc-600">
            Click &quot;Compare with Random Input&quot; to generate a random 33-byte input and compare its SHA-256 fractal analysis against the pubkey.
            <div className="text-[8px] text-cyan-500/50 font-mono mt-1">
              Key test: if SHA-256(pubkey) ≠ SHA-256(random) in fractal structure → FOUND SOMETHING
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// --- Section D: Pubkey Sequence Scanner ---

function SequenceScannerSection({
  targetPubkeyHex,
  targetPoint,
  onFindings,
}: {
  targetPubkeyHex: string | null;
  targetPoint: Point | null;
  onFindings: (findings: Finding[]) => void;
}) {
  const [neighborCount, setNeighborCount] = useState(10);
  const [results, setResults] = useState<NeighborResult[]>([]);
  const [isScanning, setIsScanning] = useState(false);
  const [scanProgress, setScanProgress] = useState(0);
  const [selectedOffset, setSelectedOffset] = useState<number | null>(null);

  const handleScan = useCallback(() => {
    if (!targetPubkeyHex || !targetPoint) return;

    setIsScanning(true);
    setScanProgress(0);
    setResults([]);

    setTimeout(() => {
      try {
        const G = getGenerator();
        const targetSha256 = sha256OfCompressedPubkey(targetPubkeyHex);
        const newResults: NeighborResult[] = [];

        // Compute P itself (offset 0)
        const targetBlock = pubkeyToSha256Block(targetPubkeyHex);
        const targetDimScore = quickDimensionScore(targetBlock);
        newResults.push({
          offset: 0,
          compressedHex: targetPubkeyHex,
          sha256Hex: targetSha256,
          hammingWeight: hammingWeightHex(targetSha256),
          dimScore: targetDimScore,
          hammingDistFromTarget: 0,
          diffusionProfile: quickDiffusionProfile(targetBlock),
        });

        // Compute neighbors
        for (let n = 1; n <= neighborCount; n++) {
          // P + nG
          const nG = scalarMultiply(BigInt(n), G);
          if (nG) {
            const plusN = pointAdd(targetPoint, nG);
            if (plusN) {
              const compHex = pointToCompressedHex(plusN);
              const sha256 = sha256OfCompressedPubkey(compHex);
              const block = pubkeyToSha256Block(compHex);
              newResults.push({
                offset: n,
                compressedHex: compHex,
                sha256Hex: sha256,
                hammingWeight: hammingWeightHex(sha256),
                dimScore: quickDimensionScore(block),
                hammingDistFromTarget: hammingWeightHex(targetSha256) !== hammingWeightHex(sha256)
                  ? (() => {
                      const b1 = hexToBytes(targetSha256);
                      const b2 = hexToBytes(sha256);
                      let d = 0;
                      for (let i = 0; i < 32; i++) d += popcount32((b1[i] ^ b2[i]) >>> 0);
                      return d;
                    })()
                  : 0,
                diffusionProfile: quickDiffusionProfile(block),
              });
            }
          }

          // P - nG
          const negN = scalarMultiply(CURVE_N - BigInt(n), G);
          if (negN) {
            const minusN = pointAdd(targetPoint, negN);
            if (minusN) {
              const compHex = pointToCompressedHex(minusN);
              const sha256 = sha256OfCompressedPubkey(compHex);
              const block = pubkeyToSha256Block(compHex);
              newResults.push({
                offset: -n,
                compressedHex: compHex,
                sha256Hex: sha256,
                hammingWeight: hammingWeightHex(sha256),
                dimScore: quickDimensionScore(block),
                hammingDistFromTarget: (() => {
                  const b1 = hexToBytes(targetSha256);
                  const b2 = hexToBytes(sha256);
                  let d = 0;
                  for (let i = 0; i < 32; i++) d += popcount32((b1[i] ^ b2[i]) >>> 0);
                  return d;
                })(),
                diffusionProfile: quickDiffusionProfile(block),
              });
            }
          }

          setScanProgress(Math.round((n / neighborCount) * 100));
        }

        // Sort by offset
        newResults.sort((a, b) => a.offset - b.offset);
        setResults(newResults);

        // Analyze for patterns
        const findings: Finding[] = [];

        // Check if dimScore correlates with offset (linear trend)
        if (newResults.length > 4) {
          const offsets = newResults.map(r => r.offset);
          const dimScores = newResults.map(r => r.dimScore);
          const meanX = offsets.reduce((a, b) => a + b, 0) / offsets.length;
          const meanY = dimScores.reduce((a, b) => a + b, 0) / dimScores.length;
          let num = 0, denX = 0, denY = 0;
          for (let i = 0; i < offsets.length; i++) {
            const dx = offsets[i] - meanX;
            const dy = dimScores[i] - meanY;
            num += dx * dy;
            denX += dx * dx;
            denY += dy * dy;
          }
          const correlation = denX > 0 && denY > 0 ? num / Math.sqrt(denX * denY) : 0;

          if (Math.abs(correlation) > 0.7) {
            findings.push({
              type: "structure",
              section: "Sequence Scanner",
              message: `⚠ CORRELATION DETECTED: fractal dimension correlates with EC offset (r=${correlation.toFixed(3)})`,
              details: `As we move along the curve from the target, the fractal dimension changes predictably. This is exploitable structure!`,
            });
          } else {
            findings.push({
              type: "random",
              section: "Sequence Scanner",
              message: `No correlation between EC offset and fractal dimension (r=${correlation.toFixed(3)})`,
              details: `Fractal dimension appears independent of position on the curve — SHA-256 behaves randomly`,
            });
          }

          // Check for periodic patterns in dimScore
          const dimVariance = dimScores.reduce((s, d) => s + (d - meanY) ** 2, 0) / dimScores.length;
          const expectedVariance = 0.002; // expected random variance
          if (dimVariance < expectedVariance * 0.1) {
            findings.push({
              type: "structure",
              section: "Sequence Scanner",
              message: `⚠ UNUSUALLY LOW VARIANCE in dimension scores (σ²=${dimVariance.toFixed(6)})`,
              details: `All neighbors produce nearly identical fractal dimensions — SHA-256 is not distinguishing between EC neighbors`,
            });
          }

          // Check Hamming distances for anomalies
          const avgHamming = newResults.filter(r => r.offset !== 0).reduce((s, r) => s + r.hammingDistFromTarget, 0) / (newResults.length - 1);
          if (Math.abs(avgHamming - 128) > 20) {
            findings.push({
              type: "anomaly",
              section: "Sequence Scanner",
              message: `⚠ Average Hamming distance to target is ${avgHamming.toFixed(1)} (expected ~128)`,
              details: `SHA-256(pubkey_neighbors) are too ${avgHamming < 128 ? "similar" : "different"} to the target hash`,
            });
          }
        }

        onFindings(findings);
      } catch (e) {
        console.error("Sequence scanner error:", e);
      }
      setIsScanning(false);
    }, 50);
  }, [targetPubkeyHex, targetPoint, neighborCount, onFindings]);

  // Chart data for dimension score
  const dimChartData = useMemo(() => {
    return results.map(r => ({
      offset: r.offset,
      dimScore: r.dimScore,
      hammingWeight: r.hammingWeight / 256, // normalize to 0-1
      hammingDist: r.hammingDistFromTarget,
    }));
  }, [results]);

  // Diffusion profile for selected offset
  const selectedDiffusion = useMemo(() => {
    if (selectedOffset === null || results.length === 0) return null;
    const r = results.find(r => r.offset === selectedOffset);
    if (!r) return null;
    return r.diffusionProfile.map((d, round) => ({
      round,
      distance: d,
      expected: 128,
    }));
  }, [results, selectedOffset]);

  // Correlation coefficient
  const correlationCoeff = useMemo(() => {
    if (results.length < 3) return 0;
    const offsets = results.map(r => r.offset);
    const dimScores = results.map(r => r.dimScore);
    const meanX = offsets.reduce((a, b) => a + b, 0) / offsets.length;
    const meanY = dimScores.reduce((a, b) => a + b, 0) / dimScores.length;
    let num = 0, denX = 0, denY = 0;
    for (let i = 0; i < offsets.length; i++) {
      const dx = offsets[i] - meanX;
      const dy = dimScores[i] - meanY;
      num += dx * dy;
      denX += dx * dx;
      denY += dy * dy;
    }
    return denX > 0 && denY > 0 ? num / Math.sqrt(denX * denY) : 0;
  }, [results]);

  return (
    <Card className="bg-zinc-900/60 border-zinc-800">
      <CardHeader className="pb-2 pt-3 px-4">
        <CardTitle className="text-xs font-semibold text-zinc-400 uppercase tracking-wider flex items-center gap-2">
          <ScanLine className="h-3.5 w-3.5 text-amber-400" />
          Pubkey Sequence Scanner
          <span className="text-[8px] text-zinc-600 font-mono normal-case ml-2">
            — Does fractal dimension vary predictably along the curve?
          </span>
        </CardTitle>
      </CardHeader>
      <CardContent className="px-4 pb-4 space-y-3">
        {/* Controls */}
        <div className="flex items-center gap-3 flex-wrap">
          <div className="flex items-center gap-2">
            <label className="text-[9px] text-zinc-500 uppercase shrink-0">Neighbors:</label>
            <Select value={String(neighborCount)} onValueChange={(v) => setNeighborCount(parseInt(v))}>
              <SelectTrigger className="w-16 h-7 text-[10px] bg-zinc-950 border-zinc-700">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="5">5</SelectItem>
                <SelectItem value="10">10</SelectItem>
                <SelectItem value="20">20</SelectItem>
                <SelectItem value="50">50</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <Button
            size="sm"
            onClick={handleScan}
            disabled={isScanning || !targetPubkeyHex || !targetPoint}
            className="h-7 text-[10px] bg-amber-600 hover:bg-amber-500 text-white"
          >
            {isScanning ? (
              <span className="flex items-center gap-1">
                <svg className="animate-spin w-3 h-3" viewBox="0 0 24 24">
                  <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
                  <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                </svg>
                Scanning... {scanProgress}%
              </span>
            ) : (
              <>
                <Zap className="h-3 w-3 mr-1" />
                Scan Neighbors
              </>
            )}
          </Button>
          {!targetPoint && targetPubkeyHex && (
            <Badge variant="outline" className="text-[8px] border-orange-500/50 text-orange-400">
              Need validated pubkey point
            </Badge>
          )}
          {results.length > 0 && (
            <Badge variant="outline" className={`text-[8px] ${Math.abs(correlationCoeff) > 0.7 ? "border-red-500/50 text-red-400" : "border-zinc-600 text-zinc-400"}`}>
              Correlation: r={correlationCoeff.toFixed(3)}
            </Badge>
          )}
        </div>

        {/* Description */}
        <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-2 text-[8px] text-zinc-500">
          Computes P±nG for n=1..{neighborCount}, then SHA-256(P±nG) for each neighbor.
          If fractal dimension varies predictably with offset → exploitable structure found.
        </div>

        {/* Dimension score chart */}
        {results.length > 0 && (
          <div className="space-y-4">
            <div>
              <div className="text-[9px] text-zinc-500 uppercase mb-1">Fractal Dimension Score vs EC Offset</div>
              <div className="h-52">
                <ResponsiveContainer width="100%" height="100%">
                  <LineChart data={dimChartData}>
                    <CartesianGrid strokeDasharray="3 3" stroke="#27272a" />
                    <XAxis dataKey="offset" tick={{ fill: "#71717a", fontSize: 9 }} label={{ value: "Offset from target", position: "insideBottom", offset: -2, style: { fill: "#71717a", fontSize: 8 } }} />
                    <YAxis domain={[0.8, 1.2]} tick={{ fill: "#71717a", fontSize: 9 }} label={{ value: "Dim Score", angle: -90, position: "insideLeft", style: { fill: "#71717a", fontSize: 8 } }} />
                    <Tooltip contentStyle={{ backgroundColor: "#18181b", border: "1px solid #3f3f46", borderRadius: 8, fontSize: 10 }} />
                    <ReferenceLine y={1.0} stroke="#10b981" strokeDasharray="5 5" strokeOpacity={0.5} label={{ value: "Random", position: "right", style: { fill: "#10b981", fontSize: 8 } }} />
                    <Line type="monotone" dataKey="dimScore" stroke="#f59e0b" strokeWidth={2} dot={{ r: 3, fill: "#f59e0b", onClick: (_, event) => {
                      // Type issue workaround
                    } }} name="Dim Score" />
                  </LineChart>
                </ResponsiveContainer>
              </div>
            </div>

            {/* Hamming distance chart */}
            <div>
              <div className="text-[9px] text-zinc-500 uppercase mb-1">SHA-256 Hamming Distance from Target</div>
              <div className="h-40">
                <ResponsiveContainer width="100%" height="100%">
                  <BarChart data={dimChartData.filter(d => d.offset !== 0)}>
                    <CartesianGrid strokeDasharray="3 3" stroke="#27272a" />
                    <XAxis dataKey="offset" tick={{ fill: "#71717a", fontSize: 9 }} />
                    <YAxis domain={[0, 256]} tick={{ fill: "#71717a", fontSize: 9 }} />
                    <Tooltip contentStyle={{ backgroundColor: "#18181b", border: "1px solid #3f3f46", borderRadius: 8, fontSize: 10 }} />
                    <ReferenceLine y={128} stroke="#10b981" strokeDasharray="5 5" strokeOpacity={0.5} label={{ value: "Expected", position: "right", style: { fill: "#10b981", fontSize: 8 } }} />
                    <Bar dataKey="hammingDist" radius={[2, 2, 0, 0]}>
                      {dimChartData.filter(d => d.offset !== 0).map((entry, i) => (
                        <Cell key={i} fill={Math.abs(entry.hammingDist - 128) > 20 ? "#ef4444" : Math.abs(entry.hammingDist - 128) > 10 ? "#f97316" : "#06b6d4"} />
                      ))}
                    </Bar>
                  </BarChart>
                </ResponsiveContainer>
              </div>
            </div>

            {/* Neighbor details table */}
            <div>
              <div className="text-[9px] text-zinc-500 uppercase mb-1">Neighbor Details</div>
              <div className="max-h-48 overflow-y-auto rounded-lg border border-zinc-800 bg-zinc-950/40">
                <div className="sticky top-0 bg-zinc-900/95 backdrop-blur-sm border-b border-zinc-800 grid grid-cols-[50px_60px_80px_60px_60px] gap-1 px-2 py-1 text-[7px] text-zinc-500 uppercase font-semibold z-10">
                  <span>Offset</span>
                  <span>Dim Score</span>
                  <span>SHA-256</span>
                  <span>Weight</span>
                  <span>HDist</span>
                </div>
                {results.map((r) => (
                  <div
                    key={r.offset}
                    className={`grid grid-cols-[50px_60px_80px_60px_60px] gap-1 px-2 py-1 border-b border-zinc-800/50 cursor-pointer hover:bg-zinc-800/40 transition-colors text-[8px] font-mono ${
                      selectedOffset === r.offset ? "bg-amber-950/20" : ""
                    }`}
                    onClick={() => setSelectedOffset(r.offset)}
                  >
                    <span className={r.offset === 0 ? "text-orange-400 font-bold" : "text-zinc-400"}>
                      {r.offset > 0 ? `+${r.offset}` : r.offset}
                    </span>
                    <span className={Math.abs(r.dimScore - 1.0) > 0.1 ? "text-red-400" : "text-emerald-400"}>
                      {r.dimScore.toFixed(3)}
                    </span>
                    <span className="text-zinc-600 truncate">{r.sha256Hex.slice(0, 12)}...</span>
                    <span className="text-zinc-500">{r.hammingWeight}</span>
                    <span className={Math.abs(r.hammingDistFromTarget - 128) > 20 ? "text-red-400" : "text-zinc-500"}>
                      {r.hammingDistFromTarget}
                    </span>
                  </div>
                ))}
              </div>
            </div>

            {/* Diffusion profile for selected neighbor */}
            {selectedDiffusion && (
              <div>
                <div className="text-[9px] text-zinc-500 uppercase mb-1">
                  Diffusion Profile — Offset {selectedOffset} (click a row above)
                </div>
                <div className="h-36">
                  <ResponsiveContainer width="100%" height="100%">
                    <AreaChart data={selectedDiffusion}>
                      <CartesianGrid strokeDasharray="3 3" stroke="#27272a" />
                      <XAxis dataKey="round" tick={{ fill: "#71717a", fontSize: 9 }} />
                      <YAxis domain={[0, 256]} tick={{ fill: "#71717a", fontSize: 9 }} />
                      <Tooltip contentStyle={{ backgroundColor: "#18181b", border: "1px solid #3f3f46", borderRadius: 8, fontSize: 10 }} />
                      <ReferenceLine y={128} stroke="#10b981" strokeDasharray="3 3" strokeOpacity={0.3} />
                      <Area type="monotone" dataKey="distance" stroke="#f59e0b" fill="#f59e0b" fillOpacity={0.1} strokeWidth={1.5} name="Hamming Dist" />
                    </AreaChart>
                  </ResponsiveContainer>
                </div>
              </div>
            )}
          </div>
        )}

        {!targetPoint && !isScanning && (
          <div className="bg-zinc-950/40 border border-zinc-800/50 rounded-lg p-4 text-center text-[10px] text-zinc-600">
            Select and validate a puzzle pubkey first to enable the sequence scanner.
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// --- Section E: Findings Summary ---

function FindingsSummarySection({
  findings,
  targetPubkeyHex,
}: {
  findings: Finding[];
  targetPubkeyHex: string | null;
}) {
  const [copied, setCopied] = useState(false);

  const handleExport = useCallback(() => {
    const exportData = {
      timestamp: new Date().toISOString(),
      targetPubkey: targetPubkeyHex,
      findings: findings.map(f => ({
        type: f.type,
        section: f.section,
        message: f.message,
        details: f.details || null,
      })),
      summary: findings.length === 0
        ? "No analysis run yet"
        : findings.some(f => f.type === "anomaly" || f.type === "structure")
          ? "STRUCTURAL ANOMALY DETECTED — SHA-256 may behave differently on this input"
          : "SHA-256 appears random on this input (no exploitable structure found)",
    };

    const blob = new Blob([JSON.stringify(exportData, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `vortex-prime-findings-${Date.now()}.json`;
    a.click();
    URL.revokeObjectURL(url);
  }, [findings, targetPubkeyHex]);

  const handleCopy = useCallback(() => {
    const text = findings.map(f => `[${f.type.toUpperCase()}] ${f.section}: ${f.message}${f.details ? `\n  → ${f.details}` : ""}`).join("\n\n");
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }, [findings]);

  const hasAnomalies = findings.some(f => f.type === "anomaly" || f.type === "structure");
  const hasOnlyRandom = findings.length > 0 && findings.every(f => f.type === "random" || f.type === "info");

  return (
    <Card className="bg-zinc-900/60 border-zinc-800">
      <CardHeader className="pb-2 pt-3 px-4">
        <CardTitle className="text-xs font-semibold text-zinc-400 uppercase tracking-wider flex items-center gap-2">
          <FileText className="h-3.5 w-3.5 text-purple-400" />
          Findings Summary
          <div className="ml-auto flex items-center gap-2">
            <Button
              size="sm"
              variant="outline"
              onClick={handleCopy}
              disabled={findings.length === 0}
              className="h-6 text-[9px] border-zinc-700 text-zinc-400"
            >
              {copied ? <Check className="h-3 w-3 mr-1" /> : <Copy className="h-3 w-3 mr-1" />}
              {copied ? "Copied!" : "Copy"}
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={handleExport}
              disabled={findings.length === 0}
              className="h-6 text-[9px] border-zinc-700 text-zinc-400"
            >
              <Download className="h-3 w-3 mr-1" />
              Export JSON
            </Button>
          </div>
        </CardTitle>
      </CardHeader>
      <CardContent className="px-4 pb-4 space-y-3">
        {/* Overall verdict */}
        {findings.length > 0 && (
          <div className={`rounded-lg p-3 border ${
            hasAnomalies
              ? "bg-red-950/30 border-red-500/30"
              : hasOnlyRandom
              ? "bg-emerald-950/20 border-emerald-500/20"
              : "bg-zinc-950/40 border-zinc-800"
          }`}>
            {hasAnomalies ? (
              <div className="flex items-start gap-2">
                <AlertTriangle className="h-4 w-4 text-red-400 mt-0.5 shrink-0" />
                <div>
                  <div className="text-[10px] text-red-400 font-semibold uppercase">
                    ⚠ Structural Anomaly Detected
                  </div>
                  <div className="text-[9px] text-red-300/70 mt-0.5">
                    SHA-256 behaves differently on this secp256k1 public key compared to random data.
                    This could reveal exploitable structure.
                  </div>
                </div>
              </div>
            ) : hasOnlyRandom ? (
              <div className="flex items-start gap-2">
                <Check className="h-4 w-4 text-emerald-400 mt-0.5 shrink-0" />
                <div>
                  <div className="text-[10px] text-emerald-400 font-semibold uppercase">
                    ✓ SHA-256 Appears Random
                  </div>
                  <div className="text-[9px] text-emerald-300/70 mt-0.5">
                    No exploitable structure found. SHA-256 treats this pubkey like random input.
                  </div>
                </div>
              </div>
            ) : (
              <div className="text-[10px] text-zinc-400">
                Analysis in progress or mixed results.
              </div>
            )}
          </div>
        )}

        {/* Findings list */}
        {findings.length > 0 ? (
          <div className="space-y-1.5 max-h-64 overflow-y-auto">
            {findings.map((finding, i) => (
              <div
                key={i}
                className={`rounded-lg p-2 border text-[9px] ${
                  finding.type === "anomaly"
                    ? "bg-red-950/20 border-red-500/20"
                    : finding.type === "structure"
                    ? "bg-orange-950/20 border-orange-500/20"
                    : finding.type === "random"
                    ? "bg-emerald-950/10 border-emerald-500/10"
                    : "bg-zinc-950/40 border-zinc-800"
                }`}
              >
                <div className="flex items-center gap-2">
                  <Badge
                    variant="outline"
                    className={`text-[7px] h-4 px-1 ${
                      finding.type === "anomaly"
                        ? "border-red-500/50 text-red-400"
                        : finding.type === "structure"
                        ? "border-orange-500/50 text-orange-400"
                        : finding.type === "random"
                        ? "border-emerald-500/50 text-emerald-400"
                        : "border-zinc-600 text-zinc-400"
                    }`}
                  >
                    {finding.type.toUpperCase()}
                  </Badge>
                  <Badge variant="outline" className="text-[7px] h-4 px-1 border-zinc-700 text-zinc-500">
                    {finding.section}
                  </Badge>
                  <span className={`font-medium ${
                    finding.type === "anomaly" ? "text-red-300" :
                    finding.type === "structure" ? "text-orange-300" :
                    finding.type === "random" ? "text-emerald-300" : "text-zinc-300"
                  }`}>
                    {finding.message}
                  </span>
                </div>
                {finding.details && (
                  <div className="text-[8px] text-zinc-500 mt-1 ml-12 font-mono">
                    → {finding.details}
                  </div>
                )}
              </div>
            ))}
          </div>
        ) : (
          <div className="bg-zinc-950/40 border border-zinc-800/50 rounded-lg p-4 text-center text-[10px] text-zinc-600">
            Run the analyses above to generate findings. The summary will appear here.
          </div>
        )}

        {/* Stats */}
        {findings.length > 0 && (
          <div className="grid grid-cols-4 gap-2">
            <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-2 text-center">
              <div className="text-[7px] text-zinc-500 uppercase">Anomalies</div>
              <div className="text-xs font-mono font-bold text-red-400">{findings.filter(f => f.type === "anomaly").length}</div>
            </div>
            <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-2 text-center">
              <div className="text-[7px] text-zinc-500 uppercase">Structure</div>
              <div className="text-xs font-mono font-bold text-orange-400">{findings.filter(f => f.type === "structure").length}</div>
            </div>
            <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-2 text-center">
              <div className="text-[7px] text-zinc-500 uppercase">Info</div>
              <div className="text-xs font-mono font-bold text-cyan-400">{findings.filter(f => f.type === "info").length}</div>
            </div>
            <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-2 text-center">
              <div className="text-[7px] text-zinc-500 uppercase">Random</div>
              <div className="text-xs font-mono font-bold text-emerald-400">{findings.filter(f => f.type === "random").length}</div>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// --- Main Component ---

export default function PuzzleAnalyzerPanel() {
  const [targetPubkeyHex, setTargetPubkeyHex] = useState<string | null>(null);
  const [targetPoint, setTargetPoint] = useState<Point | null>(null);
  const [pubkeyAnalysis, setPubkeyAnalysis] = useState<FullDiscreteAnalysis | null>(null);
  const [findings, setFindings] = useState<Finding[]>([]);

  const handleTargetChange = useCallback((pubkeyHex: string | null, point: Point | null, _puzzle: BitcoinPuzzle | null) => {
    setTargetPubkeyHex(pubkeyHex);
    setTargetPoint(point);
    setPubkeyAnalysis(null);
    setFindings([]);
  }, []);

  const handleAnalysisComplete = useCallback((analysis: FullDiscreteAnalysis) => {
    setPubkeyAnalysis(analysis);

    // Auto-generate findings from the analysis
    const newFindings: Finding[] = [];
    const anomalyRounds = analysis.dimensionProfile.filter(dp => dp.isAnomaly);
    if (anomalyRounds.length > 0) {
      const firstAnomaly = anomalyRounds[0];
      newFindings.push({
        type: "anomaly",
        section: "Fractal Analysis",
        message: `⚠ STRUCTURAL ANOMALY at round ${firstAnomaly.round}, scale ${firstAnomaly.minDimensionScale}`,
        details: `Min dimension: ${firstAnomaly.minDimension.toFixed(1)} (threshold: 200). ${anomalyRounds.length} total anomaly rounds.`,
      });
    } else {
      newFindings.push({
        type: "random",
        section: "Fractal Analysis",
        message: "No dimension anomalies detected across all 64 rounds",
        details: "All dimension estimates above 200 — SHA-256 behaves randomly on this input",
      });
    }

    const avgFlatness = analysis.walshSpectrum.reduce((s, ws) => s + ws.spectralFlatness, 0) / 64;
    if (avgFlatness < 0.95) {
      newFindings.push({
        type: "structure",
        section: "Fractal Analysis",
        message: `Spectral flatness below random threshold: ${avgFlatness.toFixed(4)}`,
        details: "Walsh-Hadamard spectrum shows non-random correlation structure",
      });
    }

    setFindings(prev => [...prev.filter(f => f.section !== "Fractal Analysis"), ...newFindings]);
  }, []);

  const handleComparisonFindings = useCallback((newFindings: Finding[]) => {
    setFindings(prev => [...prev.filter(f => f.section !== "Comparison"), ...newFindings]);
  }, []);

  const handleSequenceFindings = useCallback((newFindings: Finding[]) => {
    setFindings(prev => [...prev.filter(f => f.section !== "Sequence Scanner"), ...newFindings]);
  }, []);

  return (
    <div className="space-y-4">
      {/* Research context */}
      <div className="bg-zinc-950/80 border border-zinc-800/50 rounded-lg p-3 flex items-start gap-2">
        <Search className="h-4 w-4 text-amber-400 mt-0.5 shrink-0" />
        <div className="text-[10px] text-zinc-400 leading-relaxed">
          <span className="text-amber-400 font-semibold">Puzzle Analyzer:</span>{" "}
          Does SHA-256 behave differently when processing a secp256k1 public key vs random data?
          Input a puzzle&apos;s public key, analyze its SHA-256 fractal structure, compare against random,
          and scan neighboring pubkeys for patterns. Any anomaly could reveal exploitable structure.
        </div>
      </div>

      {/* Section A: Target Input */}
      <TargetInputSection onTargetChange={handleTargetChange} />

      {/* Section B: SHA-256 Fractal Analysis */}
      <FractalAnalysisSection
        targetPubkeyHex={targetPubkeyHex}
        onAnalysisComplete={handleAnalysisComplete}
      />

      {/* Section C: Comparison */}
      <ComparisonSection
        targetPubkeyHex={targetPubkeyHex}
        pubkeyAnalysis={pubkeyAnalysis}
        onFindings={handleComparisonFindings}
      />

      {/* Section D: Sequence Scanner */}
      <SequenceScannerSection
        targetPubkeyHex={targetPubkeyHex}
        targetPoint={targetPoint}
        onFindings={handleSequenceFindings}
      />

      {/* Section E: Findings Summary */}
      <FindingsSummarySection
        findings={findings}
        targetPubkeyHex={targetPubkeyHex}
      />
    </div>
  );
}
