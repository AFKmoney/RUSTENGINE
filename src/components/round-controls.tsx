'use client';

import React from "react";
import { Button } from "@/components/ui/button";
import { Slider } from "@/components/ui/slider";
import {
  Play,
  Pause,
  SkipBack,
  SkipForward,
  ChevronsLeft,
  ChevronsRight,
  Gauge,
} from "lucide-react";

interface RoundControlsProps {
  currentRound: number;
  totalRounds: number;
  isPlaying: boolean;
  speed: number; // 1, 2, or 5
  onRoundChange: (round: number) => void;
  onPlayPause: () => void;
  onStepForward: () => void;
  onStepBackward: () => void;
  onSpeedChange: (speed: number) => void;
}

const SPEED_OPTIONS = [0.5, 1, 2, 5];

export default function RoundControls({
  currentRound,
  totalRounds,
  isPlaying,
  speed,
  onRoundChange,
  onPlayPause,
  onStepForward,
  onStepBackward,
  onSpeedChange,
}: RoundControlsProps) {
  return (
    <div className="space-y-3">
      {/* Round slider */}
      <div className="flex items-center gap-4">
        <span className="text-xs font-mono text-zinc-500 w-8">R0</span>
        <Slider
          value={[currentRound]}
          min={0}
          max={totalRounds - 1}
          step={1}
          onValueChange={(v) => onRoundChange(v[0])}
          className="flex-1"
        />
        <span className="text-xs font-mono text-zinc-500 w-10 text-right">
          R{totalRounds - 1}
        </span>
      </div>

      {/* Controls row */}
      <div className="flex items-center justify-between gap-2">
        {/* Playback buttons */}
        <div className="flex items-center gap-1">
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8 text-zinc-400 hover:text-emerald-400"
            onClick={() => onRoundChange(0)}
            title="First round"
          >
            <ChevronsLeft className="h-4 w-4" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8 text-zinc-400 hover:text-emerald-400"
            onClick={onStepBackward}
            disabled={currentRound === 0}
            title="Step backward"
          >
            <SkipBack className="h-4 w-4" />
          </Button>
          <Button
            variant="outline"
            size="icon"
            className="h-9 w-9 border-emerald-500/50 text-emerald-400 hover:bg-emerald-500/20"
            onClick={onPlayPause}
            title={isPlaying ? "Pause" : "Play"}
          >
            {isPlaying ? (
              <Pause className="h-4 w-4" />
            ) : (
              <Play className="h-4 w-4 ml-0.5" />
            )}
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8 text-zinc-400 hover:text-emerald-400"
            onClick={onStepForward}
            disabled={currentRound === totalRounds - 1}
            title="Step forward"
          >
            <SkipForward className="h-4 w-4" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8 text-zinc-400 hover:text-emerald-400"
            onClick={() => onRoundChange(totalRounds - 1)}
            title="Last round"
          >
            <ChevronsRight className="h-4 w-4" />
          </Button>
        </div>

        {/* Round indicator */}
        <div className="font-mono text-sm text-zinc-300">
          <span className="text-emerald-400 font-bold text-lg">
            {currentRound}
          </span>
          <span className="text-zinc-600"> / {totalRounds - 1}</span>
        </div>

        {/* Speed control */}
        <div className="flex items-center gap-1">
          <Gauge className="h-3.5 w-3.5 text-zinc-500 mr-1" />
          {SPEED_OPTIONS.map((s) => (
            <Button
              key={s}
              variant={speed === s ? "default" : "ghost"}
              size="sm"
              className={`h-7 px-2 text-xs font-mono ${
                speed === s
                  ? "bg-emerald-600 text-white hover:bg-emerald-700"
                  : "text-zinc-500 hover:text-emerald-400"
              }`}
              onClick={() => onSpeedChange(s)}
            >
              {s}x
            </Button>
          ))}
        </div>
      </div>
    </div>
  );
}
