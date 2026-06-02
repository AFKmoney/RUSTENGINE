'use client';

import React, { useMemo, useCallback } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { getBit, flipBit } from "@/lib/sha256-engine";
import { Shuffle, Circle, Bitcoin, Copy } from "lucide-react";

interface InputPanelProps {
  inputBlock: Uint8Array;
  flippedBitIndex: number | null;
  onInputBlockChange: (block: Uint8Array) => void;
  onFlipBit: (bitIndex: number | null) => void;
}

function bytesToHex(block: Uint8Array): string {
  return Array.from(block)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

function hexToBytes(hex: string): Uint8Array | null {
  const clean = hex.replace(/\s/g, "");
  if (clean.length !== 128) return null;
  try {
    const bytes = new Uint8Array(64);
    for (let i = 0; i < 64; i++) {
      bytes[i] = parseInt(clean.slice(i * 2, i * 2 + 2), 16);
    }
    return bytes;
  } catch {
    return null;
  }
}

function generateRandomBlock(): Uint8Array {
  const block = new Uint8Array(64);
  crypto.getRandomValues(block);
  return block;
}

function generateAllZeros(): Uint8Array {
  return new Uint8Array(64);
}

function generateBitcoinBlockHeader(): Uint8Array {
  // Example Bitcoin block header (block #1 structure - simplified/illustrative)
  const hex =
    "01000000" + // Version
    "0000000000000000000000000000000000000000000000000000000000000000" + // Prev block hash
    "3ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a" + // Merkle root
    "29ab5f49" + // Timestamp
    "ffff001d" + // Bits
    "1dac2b7c"; // Nonce
  const bytes = new Uint8Array(64);
  for (let i = 0; i < 64; i++) {
    bytes[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return bytes;
}

export default function InputPanel({
  inputBlock,
  flippedBitIndex,
  onInputBlockChange,
  onFlipBit,
}: InputPanelProps) {
  const hexString = useMemo(() => bytesToHex(inputBlock), [inputBlock]);

  const handleHexInput = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const val = e.target.value;
      const bytes = hexToBytes(val);
      if (bytes) {
        onInputBlockChange(bytes);
      }
    },
    [onInputBlockChange]
  );

  const handleBitCellClick = useCallback(
    (bitIndex: number) => {
      if (flippedBitIndex === bitIndex) {
        onFlipBit(null); // Un-flip
      } else {
        onFlipBit(bitIndex);
      }
    },
    [flippedBitIndex, onFlipBit]
  );

  // Display the input block as a 16×16 bit grid (256 bits = 64 bytes)
  // We'll show it as 8 rows of 32 bits for consistency
  return (
    <Card className="bg-zinc-900/80 border-zinc-800">
      <CardHeader className="pb-2 pt-3 px-4">
        <CardTitle className="text-xs font-semibold text-zinc-400 uppercase tracking-wider flex items-center gap-2">
          Input Block (512-bit)
          {flippedBitIndex !== null && (
            <span className="text-cyan-400 font-mono text-[10px] normal-case">
              Flipped bit {flippedBitIndex}
            </span>
          )}
        </CardTitle>
      </CardHeader>
      <CardContent className="px-4 pb-3 space-y-3">
        {/* Preset buttons */}
        <div className="flex flex-wrap gap-2">
          <Button
            variant="outline"
            size="sm"
            className="text-xs border-zinc-700 text-zinc-400 hover:text-emerald-400 hover:border-emerald-600"
            onClick={() => onInputBlockChange(generateAllZeros())}
          >
            <Circle className="h-3 w-3 mr-1" />
            All Zeros
          </Button>
          <Button
            variant="outline"
            size="sm"
            className="text-xs border-zinc-700 text-zinc-400 hover:text-emerald-400 hover:border-emerald-600"
            onClick={() => onInputBlockChange(generateRandomBlock())}
          >
            <Shuffle className="h-3 w-3 mr-1" />
            Random
          </Button>
          <Button
            variant="outline"
            size="sm"
            className="text-xs border-zinc-700 text-zinc-400 hover:text-orange-400 hover:border-orange-600"
            onClick={() => onInputBlockChange(generateBitcoinBlockHeader())}
          >
            <Bitcoin className="h-3 w-3 mr-1" />
            Bitcoin Header
          </Button>
        </div>

        {/* Hex input */}
        <div className="flex gap-2">
          <Input
            value={hexString}
            onChange={handleHexInput}
            className="font-mono text-[10px] bg-zinc-950 border-zinc-700 text-zinc-300 h-8"
            placeholder="128 hex characters (64 bytes)"
          />
        </div>

        {/* Bit grid - click to select which bit to flip */}
        <TooltipProvider delayDuration={150}>
          <div className="space-y-0.5">
            <div className="text-[9px] text-zinc-500 mb-1">
              Click a bit to flip it for diffusion analysis
            </div>
            {/* Column headers */}
            <div className="flex items-center gap-px">
              <div className="w-6 shrink-0" />
              <div className="flex flex-1 gap-px">
                {Array.from({ length: 32 }, (_, i) => (
                  <div
                    key={i}
                    className="flex-1 text-center text-[5px] sm:text-[7px] font-mono text-zinc-600 select-none"
                  >
                    {i}
                  </div>
                ))}
              </div>
            </div>

            {/* 8 rows × 32 bits = 256 bits */}
            {Array.from({ length: 8 }, (_, row) => (
              <div key={row} className="flex items-center gap-px">
                <div className="w-6 shrink-0 text-right pr-1 text-[7px] font-mono text-zinc-600 select-none">
                  W{row * 4}
                </div>
                <div className="flex flex-1 gap-px">
                  {Array.from({ length: 32 }, (_, col) => {
                    const bitIndex = row * 32 + col;
                    const bitValue = getBit(inputBlock, bitIndex);
                    const isFlipped = flippedBitIndex === bitIndex;

                    return (
                      <Tooltip key={col}>
                        <TooltipTrigger asChild>
                          <button
                            className={`flex-1 aspect-square min-w-[5px] sm:min-w-[8px] rounded-[1px] border transition-all cursor-pointer
                              ${
                                isFlipped
                                  ? "bg-cyan-400 border-cyan-300 shadow-[0_0_6px_rgba(6,182,212,0.6)]"
                                  : bitValue === 1
                                    ? "bg-emerald-600/50 border-emerald-500/30 hover:bg-emerald-500/70"
                                    : "bg-zinc-800/60 border-zinc-700/30 hover:bg-zinc-700/60"
                              }
                            `}
                            onClick={() => handleBitCellClick(bitIndex)}
                          />
                        </TooltipTrigger>
                        <TooltipContent
                          side="top"
                          className="bg-zinc-900 border-zinc-700 text-zinc-200 text-[10px]"
                        >
                          <span className="font-mono">
                            Bit {bitIndex} (byte {Math.floor(bitIndex / 8)}.
                            {7 - (bitIndex % 8)}): {bitValue}
                            {isFlipped && (
                              <span className="text-cyan-400"> ← Flipped</span>
                            )}
                          </span>
                        </TooltipContent>
                      </Tooltip>
                    );
                  })}
                </div>
              </div>
            ))}
          </div>
        </TooltipProvider>
      </CardContent>
    </Card>
  );
}
