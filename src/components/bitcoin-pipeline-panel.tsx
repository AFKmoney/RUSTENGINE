'use client';

import React, { useState, useMemo, useCallback } from "react";
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
} from "recharts";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Slider } from "@/components/ui/slider";
import { Progress } from "@/components/ui/progress";
import DiscreteFractalPanel from "@/components/discrete-fractal-panel";
import {
  computePipeline,
  computePipelineFromBytes,
  pubkeyToSha256Block,
  input33ToSha256Block,
  generateRandomPrivateKey,
  validatePublicKey,
  bytesToHex,
  exploreKeySpace,
  computeKeySpaceDistances,
  sha256Of33Bytes,
  generateRandomInput33,
  averageConsecutiveHamming,
} from "@/lib/bitcoin-pipeline";
import { verifySecp256k1 } from "@/lib/secp256k1";
import { computeFullDiscreteAnalysis } from "@/lib/discrete-fractal";
import type { FullDiscreteAnalysis } from "@/lib/discrete-fractal";
import type { PipelineResult, KeySpaceEntry, KeySpaceDistance } from "@/lib/bitcoin-pipeline";
import { Shuffle, Key, ArrowRight, Activity, Search, GitBranch, AlertTriangle, CheckCircle, Cpu, Zap } from "lucide-react";

// --- Section A: Key Input ---

function KeyInputSection({
  initialResult,
  onPipelineResult,
}: {
  initialResult: PipelineResult;
  onPipelineResult: (result: PipelineResult) => void;
}) {
  const [privKeyInput, setPrivKeyInput] = useState(initialResult.privateKeyHex);
  const [pubKeyInput, setPubKeyInput] = useState(initialResult.publicKeyCompressedHex);
  const [validationStatus, setValidationStatus] = useState<{
    valid: boolean;
    message: string;
  } | null>({ valid: true, message: "Public key computed from private key" });

  const secpTestResults = useMemo(() => {
    const result = verifySecp256k1();
    return { passed: result.passed, count: result.results.length };
  }, []);

  const handleFromPrivateKey = useCallback(() => {
    try {
      const clean = privKeyInput.replace(/\s/g, "").toLowerCase();
      if (clean.length !== 64) {
        setValidationStatus({ valid: false, message: "Private key must be 64 hex chars" });
        return;
      }
      const result = computePipeline(clean);
      onPipelineResult(result);
      setPubKeyInput(result.publicKeyCompressedHex);
      setValidationStatus({ valid: true, message: "Public key computed from private key" });
    } catch (e) {
      setValidationStatus({ valid: false, message: (e as Error).message });
    }
  }, [privKeyInput, onPipelineResult]);

  const handleValidatePubKey = useCallback(() => {
    try {
      const result = validatePublicKey(pubKeyInput);
      if (result.valid) {
        setValidationStatus({ valid: true, message: "Valid secp256k1 public key" });
      } else {
        setValidationStatus({ valid: false, message: result.error || "Invalid public key" });
      }
    } catch (e) {
      setValidationStatus({ valid: false, message: (e as Error).message });
    }
  }, [pubKeyInput]);

  const handleGenerateRandom = useCallback(() => {
    const randomKey = generateRandomPrivateKey();
    const hex = bytesToHex(randomKey);
    setPrivKeyInput(hex);
    try {
      const result = computePipelineFromBytes(randomKey);
      onPipelineResult(result);
      setPubKeyInput(result.publicKeyCompressedHex);
      setValidationStatus({ valid: true, message: "Random key generated" });
    } catch (e) {
      setValidationStatus({ valid: false, message: (e as Error).message });
    }
  }, [onPipelineResult]);

  return (
    <Card className="bg-zinc-900/60 border-zinc-800">
      <CardHeader className="pb-2 pt-3 px-4">
        <CardTitle className="text-xs font-semibold text-zinc-400 uppercase tracking-wider flex items-center gap-2">
          <Key className="h-3.5 w-3.5 text-emerald-400" />
          Key Input
          <Badge
            variant="outline"
            className={`text-[8px] ml-auto ${
              secpTestResults.passed
                ? "border-emerald-500/50 text-emerald-400"
                : "border-red-500/50 text-red-400"
            }`}
          >
            {secpTestResults.passed ? "✓" : "✗"} secp256k1 ({secpTestResults.count} tests)
          </Badge>
        </CardTitle>
      </CardHeader>
      <CardContent className="px-4 pb-4 space-y-3">
        {/* Private key input */}
        <div>
          <label className="text-[9px] text-zinc-500 uppercase mb-1 block">Private Key (hex, 64 chars)</label>
          <div className="flex gap-2">
            <Input
              value={privKeyInput}
              onChange={(e) => setPrivKeyInput(e.target.value)}
              className="font-mono text-[10px] bg-zinc-950 border-zinc-700 text-zinc-300 h-8 flex-1"
              placeholder="64 hex characters"
              maxLength={64}
            />
            <Button
              size="sm"
              onClick={handleFromPrivateKey}
              className="h-8 text-[10px] bg-emerald-600 hover:bg-emerald-500 text-white shrink-0"
            >
              <ArrowRight className="h-3 w-3 mr-1" />
              Compute
            </Button>
          </div>
        </div>

        {/* Public key input */}
        <div>
          <label className="text-[9px] text-zinc-500 uppercase mb-1 block">Public Key (hex, 66 or 130 chars)</label>
          <div className="flex gap-2">
            <Input
              value={pubKeyInput}
              onChange={(e) => setPubKeyInput(e.target.value)}
              className="font-mono text-[10px] bg-zinc-950 border-zinc-700 text-zinc-300 h-8 flex-1"
              placeholder="02... or 04..."
            />
            <Button
              size="sm"
              variant="outline"
              onClick={handleValidatePubKey}
              className="h-8 text-[10px] border-zinc-700 text-zinc-400 hover:text-cyan-400 hover:border-cyan-600 shrink-0"
            >
              Validate
            </Button>
          </div>
        </div>

        {/* Action buttons + status */}
        <div className="flex items-center gap-2 flex-wrap">
          <Button
            size="sm"
            variant="outline"
            onClick={handleGenerateRandom}
            className="h-7 text-[10px] border-zinc-700 text-zinc-400 hover:text-orange-400 hover:border-orange-600"
          >
            <Shuffle className="h-3 w-3 mr-1" />
            Generate Random
          </Button>

          {validationStatus && (
            <Badge
              variant="outline"
              className={`text-[8px] ${
                validationStatus.valid
                  ? "border-emerald-500/50 text-emerald-400"
                  : "border-red-500/50 text-red-400"
              }`}
            >
              {validationStatus.valid ? "✓" : "✗"} {validationStatus.message}
            </Badge>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

// --- Section B: Pipeline Visualization ---

function PipelineVisualization({ result }: { result: PipelineResult | null }) {
  if (!result) {
    return (
      <Card className="bg-zinc-900/60 border-zinc-800">
        <CardContent className="p-8 text-center text-zinc-500 text-sm">
          Enter a private key to see the pipeline
        </CardContent>
      </Card>
    );
  }

  const steps = [
    { label: "Private Key", value: result.privateKeyHex, color: "text-emerald-400", bytes: "32B" },
    { label: "PubKey (compressed)", value: result.publicKeyCompressedHex, color: "text-cyan-400", bytes: "33B" },
    { label: "SHA-256(pubkey)", value: result.sha256OfPubkey, color: "text-orange-400", bytes: "32B", highlight: true },
    { label: "SHA-256(SHA-256)", value: result.doubleSha256, color: "text-amber-400", bytes: "32B" },
  ];

  return (
    <Card className="bg-zinc-900/60 border-zinc-800">
      <CardHeader className="pb-2 pt-3 px-4">
        <CardTitle className="text-xs font-semibold text-zinc-400 uppercase tracking-wider flex items-center gap-2">
          <GitBranch className="h-3.5 w-3.5 text-cyan-400" />
          Pipeline Visualization
          <span className="text-[8px] text-zinc-600 font-mono normal-case ml-2">
            — SHA-256 step highlighted for fractal analysis
          </span>
        </CardTitle>
      </CardHeader>
      <CardContent className="px-4 pb-4">
        <div className="space-y-2">
          {/* Pipeline flow */}
          <div className="flex items-center gap-1 overflow-x-auto pb-2">
            {steps.map((step, i) => (
              <React.Fragment key={i}>
                <div
                  className={`min-w-[140px] flex-shrink-0 rounded-lg border p-2 ${
                    step.highlight
                      ? "border-orange-500/50 bg-orange-950/30 shadow-[0_0_12px_rgba(249,115,22,0.15)]"
                      : "border-zinc-800 bg-zinc-900/60"
                  }`}
                >
                  <div className="flex items-center gap-1 mb-1">
                    <span className="text-[8px] text-zinc-500 uppercase">{step.label}</span>
                    <Badge variant="outline" className="text-[7px] border-zinc-700 text-zinc-500 h-3.5 px-1">
                      {step.bytes}
                    </Badge>
                    {step.highlight && (
                      <Badge variant="outline" className="text-[7px] border-orange-500/50 text-orange-400 h-3.5 px-1">
                        ANALYZED
                      </Badge>
                    )}
                  </div>
                  <div className={`text-[8px] font-mono break-all ${step.color}`}>
                    {step.value}
                  </div>
                </div>
                {i < steps.length - 1 && (
                  <ArrowRight className="h-3.5 w-3.5 text-zinc-600 flex-shrink-0" />
                )}
              </React.Fragment>
            ))}
          </div>

          {/* Also show uncompressed pubkey SHA-256 for comparison */}
          <div className="text-[8px] font-mono text-zinc-600 mt-2">
            SHA-256(uncompressed): {result.sha256OfUncompressed}
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

// --- Section C: Fractal Analysis Comparison ---

function FractalAnalysisSection({
  pipelineResult,
}: {
  pipelineResult: PipelineResult | null;
}) {
  const [pubkeyAnalysis, setPubkeyAnalysis] = useState<FullDiscreteAnalysis | null>(null);
  const [randomAnalysis, setRandomAnalysis] = useState<FullDiscreteAnalysis | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [progress, setProgress] = useState(0);
  const [phase, setPhase] = useState<string>("");
  const [comparisonSummary, setComparisonSummary] = useState<{
    dimensionDiff: number;
    flatnessDiff: number;
    selfSimDiff: number;
    significantDifferences: string[];
  } | null>(null);
  const [activeFractalRound, setActiveFractalRound] = useState(0);

  const handleRunAnalysis = useCallback(() => {
    if (!pipelineResult) return;

    setIsLoading(true);
    setProgress(0);
    setPhase("Analyzing SHA-256 on structured input (public key)...");

    setTimeout(() => {
      try {
        const pubkeyBlock = pubkeyToSha256Block(pipelineResult.publicKeyCompressedHex);

        const pubAnalysis = computeFullDiscreteAnalysis(pubkeyBlock, (pct) => {
          setProgress(Math.round(pct * 0.5));
        });
        setPubkeyAnalysis(pubAnalysis);

        setPhase("Analyzing SHA-256 on random input...");
        setProgress(50);

        const randomInput = generateRandomInput33();
        const randomBlock = input33ToSha256Block(randomInput);

        const rndAnalysis = computeFullDiscreteAnalysis(randomBlock, (pct) => {
          setProgress(50 + Math.round(pct * 0.5));
        });
        setRandomAnalysis(rndAnalysis);

        const pubAvgDim = pubAnalysis.dimensionProfile.reduce((s, dp) => s + dp.minDimension, 0) / 64;
        const rndAvgDim = rndAnalysis.dimensionProfile.reduce((s, dp) => s + dp.minDimension, 0) / 64;

        const pubAvgFlat = pubAnalysis.walshSpectrum.reduce((s, ws) => s + ws.spectralFlatness, 0) / 64;
        const rndAvgFlat = rndAnalysis.walshSpectrum.reduce((s, ws) => s + ws.spectralFlatness, 0) / 64;

        const pubAvgSelfSim = pubAnalysis.selfSimilarity.reduce((s, ss) => s + ss.selfSimilarityScore, 0) / 64;
        const rndAvgSelfSim = rndAnalysis.selfSimilarity.reduce((s, ss) => s + ss.selfSimilarityScore, 0) / 64;

        const significantDifferences: string[] = [];
        const dimDiff = Math.abs(pubAvgDim - rndAvgDim);
        if (dimDiff > 10) significantDifferences.push(`Dimension: ${dimDiff.toFixed(1)} difference (pubkey=${pubAvgDim.toFixed(1)}, random=${rndAvgDim.toFixed(1)})`);

        const flatDiff = Math.abs(pubAvgFlat - rndAvgFlat);
        if (flatDiff > 0.02) significantDifferences.push(`Spectral Flatness: ${flatDiff.toFixed(4)} difference (pubkey=${pubAvgFlat.toFixed(4)}, random=${rndAvgFlat.toFixed(4)})`);

        const selfSimDiff = Math.abs(pubAvgSelfSim - rndAvgSelfSim);
        if (selfSimDiff > 0.05) significantDifferences.push(`Self-Similarity: ${selfSimDiff.toFixed(4)} difference (pubkey=${pubAvgSelfSim.toFixed(4)}, random=${rndAvgSelfSim.toFixed(4)})`);

        if (significantDifferences.length === 0) {
          significantDifferences.push("No significant differences detected — SHA-256 treats pubkey and random inputs equivalently");
        }

        setComparisonSummary({
          dimensionDiff: dimDiff,
          flatnessDiff: flatDiff,
          selfSimDiff: selfSimDiff,
          significantDifferences,
        });

        setProgress(100);
        setPhase("Analysis complete");
        setIsLoading(false);
      } catch (e) {
        setPhase(`Error: ${(e as Error).message}`);
        setIsLoading(false);
      }
    }, 50);
  }, [pipelineResult]);

  return (
    <Card className="bg-zinc-900/60 border-zinc-800">
      <CardHeader className="pb-2 pt-3 px-4">
        <CardTitle className="text-xs font-semibold text-zinc-400 uppercase tracking-wider flex items-center gap-2">
          <Activity className="h-3.5 w-3.5 text-orange-400" />
          SHA-256 Fractal Analysis: Structured vs Random Input
          <span className="text-[8px] text-zinc-600 font-mono normal-case ml-2">
            — pubkey (structured) vs random 33 bytes
          </span>
        </CardTitle>
      </CardHeader>
      <CardContent className="px-4 pb-4 space-y-4">
        {/* Run button + progress */}
        <div className="flex items-center gap-3 flex-wrap">
          <Button
            size="sm"
            onClick={handleRunAnalysis}
            disabled={isLoading || !pipelineResult}
            className="h-7 text-[10px] bg-orange-600 hover:bg-orange-500 text-white"
          >
            {isLoading ? (
              <span className="flex items-center gap-1">
                <svg className="animate-spin w-3 h-3" viewBox="0 0 24 24">
                  <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
                  <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                </svg>
                Computing...
              </span>
            ) : (
              <>
                <Zap className="h-3 w-3 mr-1" />
                Run Fractal Analysis
              </>
            )}
          </Button>
          {phase && (
            <span className="text-[9px] font-mono text-zinc-500">{phase}</span>
          )}
        </div>

        {isLoading && (
          <div className="space-y-1">
            <Progress value={progress} className="h-1.5" />
            <div className="text-[9px] text-zinc-500 font-mono">{progress}%</div>
          </div>
        )}

        {/* Comparison summary */}
        {comparisonSummary && (
          <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-3 space-y-2">
            <div className="text-[9px] text-zinc-400 uppercase font-semibold">Comparison Results</div>
            <div className="grid grid-cols-3 gap-2">
              <div className="text-center p-2 bg-zinc-900/60 rounded-lg border border-zinc-800">
                <div className="text-[8px] text-zinc-500 uppercase">Dimension Δ</div>
                <div className={`text-sm font-mono font-bold ${comparisonSummary.dimensionDiff > 10 ? "text-orange-400" : "text-emerald-400"}`}>
                  {comparisonSummary.dimensionDiff.toFixed(1)}
                </div>
              </div>
              <div className="text-center p-2 bg-zinc-900/60 rounded-lg border border-zinc-800">
                <div className="text-[8px] text-zinc-500 uppercase">Flatness Δ</div>
                <div className={`text-sm font-mono font-bold ${comparisonSummary.flatnessDiff > 0.02 ? "text-orange-400" : "text-emerald-400"}`}>
                  {comparisonSummary.flatnessDiff.toFixed(4)}
                </div>
              </div>
              <div className="text-center p-2 bg-zinc-900/60 rounded-lg border border-zinc-800">
                <div className="text-[8px] text-zinc-500 uppercase">Self-Sim Δ</div>
                <div className={`text-sm font-mono font-bold ${comparisonSummary.selfSimDiff > 0.05 ? "text-orange-400" : "text-emerald-400"}`}>
                  {comparisonSummary.selfSimDiff.toFixed(4)}
                </div>
              </div>
            </div>
            <div className="space-y-1">
              {comparisonSummary.significantDifferences.map((diff, i) => (
                <div
                  key={i}
                  className={`text-[9px] font-mono flex items-start gap-1 ${
                    diff.includes("No significant") ? "text-emerald-400" : "text-orange-400"
                  }`}
                >
                  {diff.includes("No significant") ? (
                    <CheckCircle className="h-3 w-3 mt-0.5 shrink-0" />
                  ) : (
                    <AlertTriangle className="h-3 w-3 mt-0.5 shrink-0" />
                  )}
                  {diff}
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Side-by-side fractal panels */}
        {pubkeyAnalysis && randomAnalysis && (
          <div className="grid grid-cols-1 xl:grid-cols-2 gap-4">
            <div>
              <div className="text-[9px] text-zinc-500 uppercase font-semibold mb-2 flex items-center gap-1">
                <span className="w-2 h-2 rounded-full bg-orange-400 inline-block" />
                SHA-256 on Public Key (Structured Input)
              </div>
              <DiscreteFractalPanel
                analysis={pubkeyAnalysis}
                currentRound={activeFractalRound}
                onRoundChange={setActiveFractalRound}
              />
            </div>
            <div>
              <div className="text-[9px] text-zinc-500 uppercase font-semibold mb-2 flex items-center gap-1">
                <span className="w-2 h-2 rounded-full bg-zinc-400 inline-block" />
                SHA-256 on Random Input
              </div>
              <DiscreteFractalPanel
                analysis={randomAnalysis}
                currentRound={activeFractalRound}
                onRoundChange={setActiveFractalRound}
              />
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// --- Section D: Key Space Explorer ---

/** Compute initial key space data synchronously for lazy state init */
function computeInitialKeySpace(): {
  entries: KeySpaceEntry[];
  distances: KeySpaceDistance[];
  avgDistance: number;
  randomAvgDistance: number;
} {
  const entries = exploreKeySpace(1, 50);
  const distances = computeKeySpaceDistances(entries);
  const avgDistance = distances.length > 0
    ? distances.reduce((s, d) => s + d.hammingDistance, 0) / distances.length
    : 0;

  const randomHashes: string[] = [];
  for (let i = 0; i < 50; i++) {
    const rnd = generateRandomInput33();
    randomHashes.push(sha256Of33Bytes(rnd));
  }
  const randomAvgDistance = averageConsecutiveHamming(randomHashes);

  return { entries, distances, avgDistance, randomAvgDistance };
}

function KeySpaceExplorer() {
  const [keyRange, setKeyRange] = useState<[number, number]>([1, 50]);
  const [isComputing, setIsComputing] = useState(false);

  // Initialize all data using lazy state init
  const [keySpaceData, setKeySpaceData] = useState(computeInitialKeySpace);
  const entries = keySpaceData.entries;
  const distances = keySpaceData.distances;
  const avgDistance = keySpaceData.avgDistance;
  const randomAvgDistance = keySpaceData.randomAvgDistance;

  const handleExplore = useCallback(() => {
    setIsComputing(true);

    setTimeout(() => {
      try {
        const result = exploreKeySpace(keyRange[0], keyRange[1] - keyRange[0] + 1);
        const dists = computeKeySpaceDistances(result);
        const avg = dists.length > 0
          ? dists.reduce((s, d) => s + d.hammingDistance, 0) / dists.length
          : 0;

        const randomHashes: string[] = [];
        for (let i = 0; i < Math.min(result.length, 50); i++) {
          const rnd = generateRandomInput33();
          randomHashes.push(sha256Of33Bytes(rnd));
        }
        const rndAvg = averageConsecutiveHamming(randomHashes);

        setKeySpaceData({
          entries: result,
          distances: dists,
          avgDistance: avg,
          randomAvgDistance: rndAvg,
        });
      } catch (e) {
        console.error("Key space exploration error:", e);
      }

      setIsComputing(false);
    }, 50);
  }, [keyRange]);

  const distanceChartData = useMemo(() => {
    return distances.map((d) => ({
      keyRange: `${d.key1}-${d.key2}`,
      distance: d.hammingDistance,
      isCloser: d.isCloserThanRandom,
    }));
  }, [distances]);

  const hashHeatmapData = useMemo(() => {
    return entries.map((e) => {
      const bits: number[] = [];
      for (let i = 0; i < 8; i++) {
        const nybble = parseInt(e.sha256Hex[i], 16);
        bits.push((nybble >> 3) & 1);
        bits.push((nybble >> 2) & 1);
        bits.push((nybble >> 1) & 1);
        bits.push(nybble & 1);
      }
      return { key: e.privateKeyInt, bits, hash: e.sha256Hex };
    });
  }, [entries]);

  return (
    <Card className="bg-zinc-900/60 border-zinc-800">
      <CardHeader className="pb-2 pt-3 px-4">
        <CardTitle className="text-xs font-semibold text-zinc-400 uppercase tracking-wider flex items-center gap-2">
          <Search className="h-3.5 w-3.5 text-cyan-400" />
          Key Space Explorer
          <span className="text-[8px] text-zinc-600 font-mono normal-case ml-2">
            — SHA-256(pubkey) distance as private key increments
          </span>
        </CardTitle>
      </CardHeader>
      <CardContent className="px-4 pb-4 space-y-4">
        {/* Range controls */}
        <div className="flex items-center gap-4 flex-wrap">
          <div className="flex items-center gap-2">
            <label className="text-[9px] text-zinc-500">Range:</label>
            <span className="text-[10px] font-mono text-cyan-400">{keyRange[0]}</span>
            <span className="text-[9px] text-zinc-600">to</span>
            <span className="text-[10px] font-mono text-cyan-400">{keyRange[1]}</span>
          </div>
          <Slider
            value={keyRange}
            onValueChange={(v) => setKeyRange([v[0], v[1]])}
            min={1}
            max={500}
            step={1}
            className="w-48"
          />
          <Button
            size="sm"
            onClick={handleExplore}
            disabled={isComputing}
            className="h-7 text-[10px] bg-cyan-600 hover:bg-cyan-500 text-white"
          >
            {isComputing ? "Computing..." : "Explore"}
          </Button>
        </div>

        {/* Summary stats */}
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-2">
          <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-2 text-center">
            <div className="text-[8px] text-zinc-500 uppercase">Keys Explored</div>
            <div className="text-sm font-mono font-bold text-cyan-400">{entries.length}</div>
          </div>
          <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-2 text-center">
            <div className="text-[8px] text-zinc-500 uppercase">Avg Hamming (pubkey)</div>
            <div className={`text-sm font-mono font-bold ${Math.abs(avgDistance - 128) < 10 ? "text-emerald-400" : "text-orange-400"}`}>
              {avgDistance.toFixed(1)}
            </div>
          </div>
          <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-2 text-center">
            <div className="text-[8px] text-zinc-500 uppercase">Avg Hamming (random)</div>
            <div className="text-sm font-mono font-bold text-zinc-400">
              {randomAvgDistance.toFixed(1)}
            </div>
          </div>
          <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-2 text-center">
            <div className="text-[8px] text-zinc-500 uppercase">Structure Detected</div>
            <div className={`text-sm font-mono font-bold ${Math.abs(avgDistance - randomAvgDistance) > 5 ? "text-orange-400" : "text-emerald-400"}`}>
              {Math.abs(avgDistance - randomAvgDistance) > 5 ? "YES" : "NO"}
            </div>
          </div>
        </div>

        {/* Hamming distance chart */}
        {distanceChartData.length > 0 && (
          <div>
            <div className="text-[9px] text-zinc-500 uppercase mb-1">
              Consecutive SHA-256(pubkey) Hamming Distances
            </div>
            <div className="h-48">
              <ResponsiveContainer width="100%" height="100%">
                <BarChart data={distanceChartData}>
                  <CartesianGrid strokeDasharray="3 3" stroke="#27272a" />
                  <XAxis dataKey="keyRange" tick={{ fill: "#71717a", fontSize: 8 }} />
                  <YAxis domain={[80, 180]} tick={{ fill: "#71717a", fontSize: 9 }} />
                  <Tooltip
                    contentStyle={{ backgroundColor: "#18181b", border: "1px solid #3f3f46", borderRadius: 8, fontSize: 10 }}
                  />
                  <ReferenceLine y={128} stroke="#10b981" strokeDasharray="5 5" label={{ value: "Expected (random)", position: "right", style: { fill: "#10b981", fontSize: 9 } }} />
                  <Bar dataKey="distance" radius={[2, 2, 0, 0]}>
                    {distanceChartData.map((entry, index) => (
                      <Cell
                        key={index}
                        fill={entry.isCloser ? "#f97316" : "#06b6d4"}
                      />
                    ))}
                  </Bar>
                </BarChart>
              </ResponsiveContainer>
            </div>
          </div>
        )}

        {/* Hash output heatmap — first 32 bits of each SHA-256 */}
        {hashHeatmapData.length > 0 && (
          <div>
            <div className="text-[9px] text-zinc-500 uppercase mb-1">
              SHA-256(pubkey) First 32 Bits — Consecutive Keys
            </div>
            <div className="overflow-x-auto">
              <div className="inline-block">
                <div className="flex gap-px mb-0.5 pl-8">
                  {Array.from({ length: 32 }, (_, i) => (
                    <div key={i} className="w-2.5 text-center text-[5px] text-zinc-700 font-mono">
                      {i % 8 === 0 ? Math.floor(i / 8) : ""}
                    </div>
                  ))}
                </div>
                <div className="flex flex-col gap-px max-h-64 overflow-y-auto">
                  {hashHeatmapData.map((entry) => (
                    <div key={entry.key} className="flex items-center gap-px">
                      <div className="w-7 text-right text-[7px] font-mono text-zinc-600 pr-1">
                        {entry.key}
                      </div>
                      {entry.bits.map((bit, i) => (
                        <div
                          key={i}
                          className="w-2.5 h-3 rounded-[1px]"
                          style={{
                            backgroundColor: bit ? "rgba(6, 182, 212, 0.7)" : "rgba(39, 39, 42, 0.6)",
                          }}
                          title={`Key ${entry.key} bit ${i}: ${bit}`}
                        />
                      ))}
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </div>
        )}

        {/* Key list (scrollable) */}
        {entries.length > 0 && (
          <div>
            <div className="text-[9px] text-zinc-500 uppercase mb-1">
              Key → Pubkey → SHA-256 Mapping
            </div>
            <div className="max-h-48 overflow-y-auto bg-zinc-950/60 border border-zinc-800 rounded-lg">
              <div className="text-[8px] font-mono">
                {entries.map((entry) => (
                  <div key={entry.privateKeyInt} className="flex items-center gap-2 px-2 py-0.5 border-b border-zinc-800/50 hover:bg-zinc-800/30">
                    <span className="text-zinc-500 w-6 shrink-0 text-right">{entry.privateKeyInt}</span>
                    <span className="text-zinc-600">→</span>
                    <span className="text-cyan-400/70 truncate">{entry.publicKeyCompressedHex.slice(0, 20)}...</span>
                    <span className="text-zinc-600">→</span>
                    <span className="text-orange-400/70 truncate">{entry.sha256Hex.slice(0, 16)}...</span>
                  </div>
                ))}
              </div>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// --- Main Component ---

export default function BitcoinPipelinePanel() {
  // Initialize pipeline result with default key=1
  const [pipelineResult, setPipelineResult] = useState<PipelineResult>(() =>
    computePipeline("0000000000000000000000000000000000000000000000000000000000000001")
  );

  const handlePipelineResult = useCallback((result: PipelineResult) => {
    setPipelineResult(result);
  }, []);

  return (
    <div className="space-y-4">
      {/* Research context banner */}
      <div className="bg-zinc-950/80 border border-zinc-800/50 rounded-lg p-3 flex items-start gap-2">
        <Cpu className="h-4 w-4 text-orange-400 mt-0.5 shrink-0" />
        <div className="text-[10px] text-zinc-400 leading-relaxed">
          <span className="text-orange-400 font-semibold">Research Mode:</span>{" "}
          This tool analyzes how SHA-256 processes secp256k1 public keys vs random data.
          We observe the SHA-256 compression function&apos;s fractal behavior on structured inputs
          (EC points) compared to unstructured inputs.{" "}
          <span className="text-zinc-600">This does NOT attempt to reverse ECDSA.</span>
        </div>
      </div>

      {/* Section A: Key Input */}
      <KeyInputSection initialResult={pipelineResult} onPipelineResult={handlePipelineResult} />

      {/* Section B: Pipeline Visualization */}
      <PipelineVisualization result={pipelineResult} />

      {/* Section D: Key Space Explorer */}
      <KeySpaceExplorer />

      {/* Section C: Fractal Analysis Comparison */}
      <FractalAnalysisSection pipelineResult={pipelineResult} />
    </div>
  );
}
