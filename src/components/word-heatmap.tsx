'use client';

import React, { useMemo } from "react";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import type { DiffusionData } from "@/lib/diffusion-analyzer";
import { WORD_NAMES } from "@/lib/diffusion-analyzer";

interface WordHeatmapProps {
  diffusion: DiffusionData[];
  currentRound: number;
  onRoundClick?: (round: number) => void;
}

function getHeatmapColor(percent: number): string {
  // Dark green -> yellow -> orange -> red gradient based on diffusion %
  if (percent === 0) return "bg-zinc-800/60";
  if (percent < 10) return "bg-emerald-900/70";
  if (percent < 20) return "bg-emerald-700/70";
  if (percent < 30) return "bg-emerald-500/70";
  if (percent < 40) return "bg-yellow-600/70";
  if (percent < 50) return "bg-orange-500/70";
  if (percent < 60) return "bg-orange-600/70";
  if (percent < 70) return "bg-red-500/70";
  if (percent < 80) return "bg-red-600/70";
  return "bg-red-700/80";
}

export default function WordHeatmap({
  diffusion,
  currentRound,
  onRoundClick,
}: WordHeatmapProps) {
  if (diffusion.length === 0) {
    return (
      <div className="h-32 flex items-center justify-center text-zinc-500 text-sm">
        No data
      </div>
    );
  }

  return (
    <TooltipProvider delayDuration={200}>
      <div className="space-y-1">
        <h3 className="text-xs font-semibold text-zinc-400 uppercase tracking-wider">
          Word-Level Heatmap
        </h3>
        <div className="overflow-x-auto">
          <div className="min-w-[400px]">
            {/* Round labels (every 4th) */}
            <div className="flex items-center gap-px mb-0.5">
              <div className="w-8 shrink-0" />
              {Array.from({ length: 64 }, (_, r) => (
                <div
                  key={r}
                  className={`flex-1 text-center text-[6px] font-mono select-none ${
                    r % 8 === 0 ? "text-zinc-500" : "text-transparent"
                  }`}
                >
                  {r}
                </div>
              ))}
            </div>

            {/* Heatmap rows */}
            {WORD_NAMES.map((name, w) => (
              <div key={name} className="flex items-center gap-px">
                <div className="w-8 shrink-0 text-right pr-1 text-[9px] font-mono font-bold text-emerald-400 select-none">
                  {name}
                </div>
                {diffusion.map((d, r) => {
                  const pct = d.wordDiffusionPercents[w];
                  const isCurrentRound = r === currentRound;

                  return (
                    <Tooltip key={r}>
                      <TooltipTrigger asChild>
                        <div
                          className={`flex-1 h-4 min-w-[5px] rounded-[1px] cursor-pointer transition-all ${getHeatmapColor(
                            pct
                          )} ${
                            isCurrentRound
                              ? "ring-1 ring-cyan-400 scale-y-125 z-10"
                              : "hover:scale-y-125 hover:z-10"
                          }`}
                          onClick={() => onRoundClick?.(r)}
                        />
                      </TooltipTrigger>
                      <TooltipContent
                        side="top"
                        className="bg-zinc-900 border-zinc-700 text-zinc-200 text-[10px]"
                      >
                        <span className="font-mono">
                          {name} @ R{r}: {pct.toFixed(1)}%
                        </span>
                      </TooltipContent>
                    </Tooltip>
                  );
                })}
              </div>
            ))}

            {/* Color legend */}
            <div className="flex items-center gap-2 pt-1 text-[8px] text-zinc-500">
              <span>0%</span>
              <div className="flex gap-px flex-1">
                <div className="flex-1 h-2 bg-zinc-800/60 rounded-l-sm" />
                <div className="flex-1 h-2 bg-emerald-900/70" />
                <div className="flex-1 h-2 bg-emerald-700/70" />
                <div className="flex-1 h-2 bg-emerald-500/70" />
                <div className="flex-1 h-2 bg-yellow-600/70" />
                <div className="flex-1 h-2 bg-orange-500/70" />
                <div className="flex-1 h-2 bg-orange-600/70" />
                <div className="flex-1 h-2 bg-red-500/70" />
                <div className="flex-1 h-2 bg-red-600/70 rounded-r-sm" />
              </div>
              <span>70%+</span>
            </div>
          </div>
        </div>
      </div>
    </TooltipProvider>
  );
}
