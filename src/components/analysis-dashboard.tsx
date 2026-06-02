'use client';

import React from "react";
import DiffusionChart from "./diffusion-chart";
import WordHeatmap from "./word-heatmap";
import type { DiffusionData } from "@/lib/diffusion-analyzer";
import { findAvalanchePoint } from "@/lib/diffusion-analyzer";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

interface AnalysisDashboardProps {
  diffusion: DiffusionData[];
  currentRound: number;
  onRoundClick?: (round: number) => void;
  flipBitIndex: number | null;
}

export default function AnalysisDashboard({
  diffusion,
  currentRound,
  onRoundClick,
  flipBitIndex,
}: AnalysisDashboardProps) {
  const currentDiffusion = diffusion[currentRound];
  const avalanchePoint =
    diffusion.length > 0 ? findAvalanchePoint(diffusion) : -1;

  const stats = currentDiffusion
    ? {
        diffusionPercent: currentDiffusion.diffusionPercent,
        activeBits: currentDiffusion.activeBitCount,
        entropy: currentDiffusion.entropy,
        avalanchePoint,
        roundsUntilAvalanche:
          avalanchePoint >= 0
            ? Math.max(0, avalanchePoint - currentRound)
            : -1,
        perWord: currentDiffusion.wordDiffusionPercents,
      }
    : null;

  return (
    <div className="space-y-4">
      {/* Statistics Panel */}
      <Card className="bg-zinc-900/80 border-zinc-800">
        <CardHeader className="pb-2 pt-3 px-4">
          <CardTitle className="text-xs font-semibold text-zinc-400 uppercase tracking-wider">
            Statistics — Round {currentRound}
          </CardTitle>
        </CardHeader>
        <CardContent className="px-4 pb-3">
          {stats ? (
            <div className="grid grid-cols-2 gap-x-4 gap-y-2">
              <div>
                <div className="text-[10px] text-zinc-500 uppercase">
                  Diffusion
                </div>
                <div className="text-lg font-mono font-bold text-emerald-400">
                  {stats.diffusionPercent.toFixed(1)}%
                </div>
              </div>
              <div>
                <div className="text-[10px] text-zinc-500 uppercase">
                  Active Bits
                </div>
                <div className="text-lg font-mono font-bold text-orange-400">
                  {stats.activeBits}
                  <span className="text-xs text-zinc-600">/256</span>
                </div>
              </div>
              <div>
                <div className="text-[10px] text-zinc-500 uppercase">
                  Entropy
                </div>
                <div className="text-sm font-mono font-bold text-cyan-400">
                  {stats.entropy.toFixed(1)}
                  <span className="text-[10px] text-zinc-600"> bits</span>
                </div>
              </div>
              <div>
                <div className="text-[10px] text-zinc-500 uppercase">
                  Avalanche Point
                </div>
                <div className="text-sm font-mono font-bold text-yellow-500">
                  {stats.avalanchePoint >= 0
                    ? `R${stats.avalanchePoint}`
                    : "N/A"}
                </div>
              </div>

              {/* Diffusion progress bar */}
              <div className="col-span-2">
                <div className="text-[10px] text-zinc-500 uppercase mb-1">
                  Diffusion Progress
                </div>
                <div className="h-2.5 bg-zinc-800 rounded-full overflow-hidden">
                  <div
                    className="h-full rounded-full transition-all duration-300"
                    style={{
                      width: `${stats.diffusionPercent}%`,
                      background: `linear-gradient(90deg, #10b981, #06b6d4, #f97316)`,
                    }}
                  />
                </div>
                <div className="flex justify-between text-[8px] text-zinc-600 mt-0.5">
                  <span>0%</span>
                  <span className="text-zinc-500">50%</span>
                  <span>100%</span>
                </div>
              </div>

              {/* Per-word mini bars */}
              <div className="col-span-2">
                <div className="text-[10px] text-zinc-500 uppercase mb-1">
                  Per-Word Diffusion
                </div>
                <div className="flex gap-1">
                  {stats.perWord.map((pct, i) => (
                    <div key={i} className="flex-1">
                      <div className="h-8 bg-zinc-800 rounded-sm overflow-hidden relative">
                        <div
                          className="absolute bottom-0 left-0 right-0 transition-all duration-300 rounded-sm"
                          style={{
                            height: `${pct}%`,
                            backgroundColor:
                              pct > 50
                                ? "#f97316"
                                : pct > 20
                                  ? "#06b6d4"
                                  : "#10b981",
                          }}
                        />
                      </div>
                      <div className="text-center text-[7px] font-mono text-zinc-600 mt-0.5">
                        {["a", "b", "c", "d", "e", "f", "g", "h"][i]}
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          ) : (
            <div className="text-sm text-zinc-500 text-center py-4">
              Select an input bit to flip to see diffusion analysis
            </div>
          )}
        </CardContent>
      </Card>

      {/* Diffusion Curve */}
      <Card className="bg-zinc-900/80 border-zinc-800">
        <CardContent className="pt-3 pb-3 px-3">
          <DiffusionChart
            diffusion={diffusion}
            currentRound={currentRound}
            onRoundClick={onRoundClick}
          />
        </CardContent>
      </Card>

      {/* Word Heatmap */}
      <Card className="bg-zinc-900/80 border-zinc-800">
        <CardContent className="pt-3 pb-3 px-3">
          <WordHeatmap
            diffusion={diffusion}
            currentRound={currentRound}
            onRoundClick={onRoundClick}
          />
        </CardContent>
      </Card>
    </div>
  );
}
