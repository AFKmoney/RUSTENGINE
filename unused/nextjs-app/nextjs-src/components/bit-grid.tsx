'use client';

import React, { useMemo, useCallback, useState } from "react";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { getWordBit } from "@/lib/sha256-engine";
import type { DiffusionData } from "@/lib/diffusion-analyzer";
import { WORD_NAMES } from "@/lib/diffusion-analyzer";

interface BitGridProps {
  roundState: {
    a: number; b: number; c: number; d: number;
    e: number; f: number; g: number; h: number;
  } | null;
  diffusion: DiffusionData | null;
  flippedBitIndex: number | null; // The input bit that was flipped (for source highlighting)
  influencedBits?: Set<string>; // Bits influenced by the selected input bit at current round
  onBitClick?: (wordIndex: number, bitIndex: number) => void;
}

export default function BitGrid({
  roundState,
  diffusion,
  flippedBitIndex,
  influencedBits,
  onBitClick,
}: BitGridProps) {
  const [hoveredCell, setHoveredCell] = useState<{
    word: number;
    bit: number;
  } | null>(null);

  const words = useMemo(() => {
    if (!roundState) return [0, 0, 0, 0, 0, 0, 0, 0];
    return [
      roundState.a, roundState.b, roundState.c, roundState.d,
      roundState.e, roundState.f, roundState.g, roundState.h,
    ];
  }, [roundState]);

  const getCellStyle = useCallback(
    (wordIdx: number, bitIdx: number) => {
      const bitValue = getWordBit(words[wordIdx], bitIdx);
      const isChanged = diffusion?.bitDiffs[wordIdx]?.[bitIdx] ?? false;
      const isInfluenced =
        influencedBits?.has(`${wordIdx}-${bitIdx}`) ?? false;
      const isSourceBit =
        flippedBitIndex !== null &&
        // Check if this state word/bit corresponds to the flipped input bit
        // For the initial state visualization, we highlight the message schedule bit
        false;

      if (isChanged) {
        return {
          bg: "bg-orange-500/80",
          border: "border-orange-400",
          glow: "shadow-[0_0_6px_rgba(249,115,22,0.5)]",
          label: "Changed",
        };
      }
      if (isInfluenced) {
        return {
          bg: "bg-cyan-500/60",
          border: "border-cyan-400",
          glow: "shadow-[0_0_6px_rgba(6,182,212,0.4)]",
          label: "Influenced",
        };
      }
      if (bitValue === 1) {
        return {
          bg: "bg-emerald-500/70",
          border: "border-emerald-400/50",
          glow: "",
          label: "1",
        };
      }
      return {
        bg: "bg-zinc-800/80",
        border: "border-zinc-700/50",
        glow: "",
        label: "0",
      };
    },
    [words, diffusion, influencedBits, flippedBitIndex]
  );

  return (
    <TooltipProvider delayDuration={100}>
      <div className="space-y-2">
        {/* Column headers */}
        <div className="flex items-center gap-0.5">
          <div className="w-8 shrink-0" /> {/* Spacer for row headers */}
          <div className="flex flex-1 gap-px">
            {Array.from({ length: 32 }, (_, i) => (
              <div
                key={i}
                className="flex-1 text-center text-[7px] sm:text-[9px] font-mono text-zinc-500 select-none"
              >
                {i}
              </div>
            ))}
          </div>
        </div>

        {/* Grid rows */}
        {WORD_NAMES.map((name, wordIdx) => (
          <div key={name} className="flex items-center gap-0.5">
            {/* Row header */}
            <div className="w-8 shrink-0 text-right pr-1 text-xs sm:text-sm font-mono font-bold text-emerald-400 select-none">
              {name}
            </div>

            {/* Bit cells */}
            <div className="flex flex-1 gap-px">
              {Array.from({ length: 32 }, (_, bitIdx) => {
                const style = getCellStyle(wordIdx, bitIdx);
                const bitValue = getWordBit(words[wordIdx], bitIdx);
                const isChanged =
                  diffusion?.bitDiffs[wordIdx]?.[bitIdx] ?? false;

                return (
                  <Tooltip key={bitIdx}>
                    <TooltipTrigger asChild>
                      <button
                        className={`
                          flex-1 aspect-square min-w-[6px] sm:min-w-[10px] md:min-w-[12px]
                          rounded-[2px] border transition-all duration-150
                          ${style.bg} ${style.border} ${style.glow}
                          ${isChanged ? "animate-pulse" : ""}
                          hover:scale-125 hover:z-10 cursor-pointer
                        `}
                        onClick={() => onBitClick?.(wordIdx, bitIdx)}
                        onMouseEnter={() =>
                          setHoveredCell({ word: wordIdx, bit: bitIdx })
                        }
                        onMouseLeave={() => setHoveredCell(null)}
                      />
                    </TooltipTrigger>
                    <TooltipContent
                      side="top"
                      className="bg-zinc-900 border-zinc-700 text-zinc-200 text-xs"
                    >
                      <div className="font-mono">
                        <span className="text-emerald-400 font-bold">
                          {name}[{bitIdx}]
                        </span>
                        <br />
                        Value: {bitValue}
                        <br />
                        {isChanged && (
                          <span className="text-orange-400">● Changed</span>
                        )}
                        {style.label === "Influenced" && (
                          <span className="text-cyan-400">● Influenced</span>
                        )}
                      </div>
                    </TooltipContent>
                  </Tooltip>
                );
              })}
            </div>
          </div>
        ))}

        {/* Legend */}
        <div className="flex flex-wrap gap-3 pt-2 text-[10px] sm:text-xs text-zinc-400">
          <div className="flex items-center gap-1">
            <div className="w-3 h-3 rounded-sm bg-zinc-800/80 border border-zinc-700/50" />
            <span>0-bit</span>
          </div>
          <div className="flex items-center gap-1">
            <div className="w-3 h-3 rounded-sm bg-emerald-500/70 border border-emerald-400/50" />
            <span>1-bit</span>
          </div>
          <div className="flex items-center gap-1">
            <div className="w-3 h-3 rounded-sm bg-orange-500/80 border border-orange-400" />
            <span>Changed</span>
          </div>
          <div className="flex items-center gap-1">
            <div className="w-3 h-3 rounded-sm bg-cyan-500/60 border border-cyan-400" />
            <span>Influenced</span>
          </div>
        </div>
      </div>
    </TooltipProvider>
  );
}
