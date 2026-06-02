'use client';

import React, { useState, useMemo, useCallback, useEffect, useRef } from "react";
import BitGrid from "@/components/bit-grid";
import RoundControls from "@/components/round-controls";
import AnalysisDashboard from "@/components/analysis-dashboard";
import InputPanel from "@/components/input-panel";
import { computeFullAnalysis, findAvalanchePoint, WORD_NAMES } from "@/lib/diffusion-analyzer";
import { verifySha256, hashToHex, sha256Full, getWordBit } from "@/lib/sha256-engine";
import type { DiffusionData } from "@/lib/diffusion-analyzer";
import type { CompressionTrace } from "@/lib/sha256-engine";

function generateRandomBlock(): Uint8Array {
  const block = new Uint8Array(64);
  crypto.getRandomValues(block);
  return block;
}

export default function Home() {
  // Core state
  const [inputBlock, setInputBlock] = useState<Uint8Array>(() => {
    const b = new Uint8Array(64);
    // Start with a recognizable pattern
    for (let i = 0; i < 64; i++) b[i] = i;
    return b;
  });
  const [flippedBitIndex, setFlippedBitIndex] = useState<number | null>(0);
  const [currentRound, setCurrentRound] = useState(0);
  const [isPlaying, setIsPlaying] = useState(false);
  const [speed, setSpeed] = useState(1);

  // Verification result - computed once via useMemo (no setState in effect)
  const verificationResult = useMemo(() => verifySha256(), []);

  // Log verification result (side-effect only, no setState)
  useEffect(() => {
    if (verificationResult.passed) {
      console.log("SHA-256 verification passed ✓");
    } else {
      console.error("SHA-256 verification failed!", verificationResult.results);
    }
  }, [verificationResult]);

  // Compute analysis whenever input or flipped bit changes
  const analysis = useMemo(() => {
    if (flippedBitIndex === null) return null;
    return computeFullAnalysis(inputBlock, flippedBitIndex);
  }, [inputBlock, flippedBitIndex]);

  const diffusion: DiffusionData[] = useMemo(
    () => analysis?.diffusion ?? [],
    [analysis]
  );

  const baseTrace: CompressionTrace | null = useMemo(
    () => analysis?.baseTrace ?? null,
    [analysis]
  );

  const modifiedTrace: CompressionTrace | null = useMemo(
    () => analysis?.modifiedTrace ?? null,
    [analysis]
  );

  // Current round state (from modified trace — the one with the bit flip)
  const roundState = useMemo(() => {
    if (!modifiedTrace || currentRound < 0 || currentRound >= 64) return null;
    return modifiedTrace.rounds[currentRound];
  }, [modifiedTrace, currentRound]);

  // Current round diffusion
  const currentDiffusion = useMemo(() => {
    if (diffusion.length === 0 || currentRound < 0 || currentRound >= 64)
      return null;
    return diffusion[currentRound];
  }, [diffusion, currentRound]);

  // Influence tracking: compute which output bits the flipped input bit influences at current round
  const influencedBits = useMemo(() => {
    if (!currentDiffusion) return new Set<string>();
    const set = new Set<string>();
    for (let w = 0; w < 8; w++) {
      for (let b = 0; b < 32; b++) {
        if (currentDiffusion.bitDiffs[w][b]) {
          set.add(`${w}-${b}`);
        }
      }
    }
    return set;
  }, [currentDiffusion]);

  // Auto-play animation
  const playIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    if (isPlaying) {
      const intervalMs = 1000 / speed;
      playIntervalRef.current = setInterval(() => {
        setCurrentRound((prev) => {
          if (prev >= 63) {
            setIsPlaying(false);
            return 63;
          }
          return prev + 1;
        });
      }, intervalMs);
    } else {
      if (playIntervalRef.current) {
        clearInterval(playIntervalRef.current);
        playIntervalRef.current = null;
      }
    }
    return () => {
      if (playIntervalRef.current) {
        clearInterval(playIntervalRef.current);
      }
    };
  }, [isPlaying, speed]);

  // Handlers
  const handlePlayPause = useCallback(() => {
    setIsPlaying((prev) => !prev);
  }, []);

  const handleStepForward = useCallback(() => {
    setCurrentRound((prev) => Math.min(prev + 1, 63));
  }, []);

  const handleStepBackward = useCallback(() => {
    setCurrentRound((prev) => Math.max(prev - 1, 0));
  }, []);

  const handleFlipBit = useCallback((bitIndex: number | null) => {
    setFlippedBitIndex(bitIndex);
    setCurrentRound(0);
    setIsPlaying(false);
  }, []);

  const handleInputBlockChange = useCallback((block: Uint8Array) => {
    setInputBlock(block);
    setCurrentRound(0);
    setIsPlaying(false);
  }, []);

  // Compute final hash for display
  const finalHash = useMemo(() => {
    try {
      // We need to show what the SHA-256 would produce with proper padding
      // But our compression works on 64-byte blocks only, so let's show the final state
      if (analysis?.baseTrace) {
        return hashToHex(analysis.baseTrace.finalState);
      }
      return "";
    } catch {
      return "";
    }
  }, [analysis]);

  const modifiedHash = useMemo(() => {
    if (analysis?.modifiedTrace) {
      return hashToHex(analysis.modifiedTrace.finalState);
    }
    return "";
  }, [analysis]);

  return (
    <div className="min-h-screen bg-[#0a0a0f] text-zinc-100 flex flex-col">
      {/* Header */}
      <header className="border-b border-zinc-800/80 bg-[#0c0c14]/90 backdrop-blur-sm sticky top-0 z-50">
        <div className="max-w-7xl mx-auto px-4 py-3 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-emerald-500 to-cyan-500 flex items-center justify-center text-black font-bold text-sm">
              ⟐
            </div>
            <div>
              <h1 className="text-base sm:text-lg font-bold tracking-tight">
                <span className="text-emerald-400">VORTEX</span>{" "}
                <span className="text-zinc-300">PRIME</span>
              </h1>
              <p className="text-[9px] sm:text-[10px] text-zinc-600 font-mono -mt-0.5">
                SHA-256 Avalanche Bit-Diffusion Visualizer
              </p>
            </div>
          </div>
          <div className="flex items-center gap-2">
            {verificationResult && (
              <div
                className={`text-[9px] font-mono px-2 py-0.5 rounded ${
                  verificationResult.passed
                    ? "bg-emerald-900/40 text-emerald-400"
                    : "bg-red-900/40 text-red-400"
                }`}
              >
                {verificationResult.passed ? "✓ SHA-256 Verified" : "✗ Verification Failed"}
              </div>
            )}
          </div>
        </div>
      </header>

      {/* Main Content */}
      <main className="flex-1 max-w-7xl mx-auto w-full px-3 sm:px-4 py-4 sm:py-6">
        <div className="grid grid-cols-1 lg:grid-cols-12 gap-4 sm:gap-6">
          {/* Left Column: Bit Grid */}
          <div className="lg:col-span-5 space-y-4">
            <div className="bg-zinc-900/60 border border-zinc-800 rounded-xl p-4">
              <div className="flex items-center justify-between mb-3">
                <h2 className="text-xs font-semibold text-zinc-400 uppercase tracking-wider">
                  State Word Grid — Round {currentRound}
                </h2>
                <div className="text-[9px] font-mono text-zinc-600">
                  {flippedBitIndex !== null
                    ? `Comparing: base vs bit ${flippedBitIndex} flipped`
                    : "No bit flip selected"}
                </div>
              </div>
              <BitGrid
                roundState={roundState}
                diffusion={currentDiffusion}
                flippedBitIndex={flippedBitIndex}
                influencedBits={influencedBits}
              />
            </div>

            {/* Message Schedule Info */}
            {analysis?.baseTrace && currentRound >= 0 && (
              <div className="bg-zinc-900/60 border border-zinc-800 rounded-xl p-4">
                <h2 className="text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-2">
                  Round {currentRound} Detail
                </h2>
                <div className="grid grid-cols-4 gap-2 text-[9px] font-mono">
                  <div className="text-zinc-500">T1</div>
                  <div className="col-span-3 text-cyan-400 break-all">
                    {(modifiedTrace?.rounds[currentRound]?.T1 >>> 0).toString(16).padStart(8, "0")}
                  </div>
                  <div className="text-zinc-500">T2</div>
                  <div className="col-span-3 text-cyan-400 break-all">
                    {(modifiedTrace?.rounds[currentRound]?.T2 >>> 0).toString(16).padStart(8, "0")}
                  </div>
                  <div className="text-zinc-500">W[{currentRound}]</div>
                  <div className="col-span-3 text-emerald-400 break-all">
                    {(analysis.baseTrace.messageSchedule[currentRound] >>> 0).toString(16).padStart(8, "0")}
                  </div>
                </div>
                <div className="mt-3 grid grid-cols-8 gap-1">
                  {WORD_NAMES.map((name, i) => {
                    const word = modifiedTrace?.rounds[currentRound]
                      ? [modifiedTrace.rounds[currentRound].a, modifiedTrace.rounds[currentRound].b, modifiedTrace.rounds[currentRound].c, modifiedTrace.rounds[currentRound].d, modifiedTrace.rounds[currentRound].e, modifiedTrace.rounds[currentRound].f, modifiedTrace.rounds[currentRound].g, modifiedTrace.rounds[currentRound].h][i]
                      : 0;
                    return (
                      <div key={name} className="text-center">
                        <div className="text-[8px] text-zinc-600">{name}</div>
                        <div className="text-[7px] font-mono text-zinc-400">
                          {(word >>> 0).toString(16).padStart(8, "0")}
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            )}
          </div>

          {/* Right Column: Analysis Dashboard */}
          <div className="lg:col-span-7">
            <AnalysisDashboard
              diffusion={diffusion}
              currentRound={currentRound}
              onRoundClick={setCurrentRound}
              flipBitIndex={flippedBitIndex}
            />
          </div>
        </div>

        {/* Round Controls - full width */}
        <div className="mt-4 sm:mt-6 bg-zinc-900/60 border border-zinc-800 rounded-xl p-4">
          <RoundControls
            currentRound={currentRound}
            totalRounds={64}
            isPlaying={isPlaying}
            speed={speed}
            onRoundChange={setCurrentRound}
            onPlayPause={handlePlayPause}
            onStepForward={handleStepForward}
            onStepBackward={handleStepBackward}
            onSpeedChange={setSpeed}
          />
        </div>

        {/* Input Panel - full width */}
        <div className="mt-4 sm:mt-6">
          <InputPanel
            inputBlock={inputBlock}
            flippedBitIndex={flippedBitIndex}
            onInputBlockChange={handleInputBlockChange}
            onFlipBit={handleFlipBit}
          />
        </div>

        {/* Hash comparison */}
        {finalHash && modifiedHash && (
          <div className="mt-4 sm:mt-6 bg-zinc-900/60 border border-zinc-800 rounded-xl p-4">
            <h2 className="text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-2">
              Compression Output Comparison
            </h2>
            <div className="space-y-1 font-mono text-[9px]">
              <div className="flex items-start gap-2">
                <span className="text-zinc-600 w-12 shrink-0">Base:</span>
                <span className="text-emerald-400/70 break-all">{finalHash}</span>
              </div>
              <div className="flex items-start gap-2">
                <span className="text-zinc-600 w-12 shrink-0">Mod:</span>
                <span className="text-orange-400/70 break-all">{modifiedHash}</span>
              </div>
              <div className="flex items-start gap-2">
                <span className="text-zinc-600 w-12 shrink-0">Diff:</span>
                <span className="break-all">
                  {finalHash.split("").map((char, i) => (
                    <span
                      key={i}
                      className={
                        char !== modifiedHash[i]
                          ? "text-red-400 font-bold"
                          : "text-zinc-700"
                      }
                    >
                      {modifiedHash[i]}
                    </span>
                  ))}
                </span>
              </div>
            </div>
          </div>
        )}
      </main>

      {/* Footer */}
      <footer className="border-t border-zinc-800/50 mt-auto">
        <div className="max-w-7xl mx-auto px-4 py-3 text-center text-[9px] text-zinc-600 font-mono">
          VORTEX PRIME — Visualizing the avalanche effect in SHA-256 compression •
          64 rounds • 256 state bits • 1 bit flip → observe the cascade
        </div>
      </footer>
    </div>
  );
}
