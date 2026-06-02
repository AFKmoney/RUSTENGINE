'use client';

import React, { useMemo } from "react";
import {
  LineChart,
  Line,
  XAxis,
  YAxis,
  CartesianGrid,
  ReferenceLine,
  Area,
  ComposedChart,
} from "recharts";
import type { DiffusionData } from "@/lib/diffusion-analyzer";

interface DiffusionChartProps {
  diffusion: DiffusionData[];
  currentRound: number;
  onRoundClick?: (round: number) => void;
}

export default function DiffusionChart({
  diffusion,
  currentRound,
  onRoundClick,
}: DiffusionChartProps) {
  const chartData = useMemo(() => {
    return diffusion.map((d) => ({
      round: d.round,
      diffusion: parseFloat(d.diffusionPercent.toFixed(1)),
      entropy: parseFloat((d.entropy / 256 * 100).toFixed(1)),
    }));
  }, [diffusion]);

  if (diffusion.length === 0) {
    return (
      <div className="h-48 flex items-center justify-center text-zinc-500 text-sm">
        No diffusion data yet
      </div>
    );
  }

  return (
    <div className="space-y-1">
      <h3 className="text-xs font-semibold text-zinc-400 uppercase tracking-wider">
        Diffusion Curve
      </h3>
      <div
        className="h-44 cursor-pointer"
        onClick={(e) => {
          if (!onRoundClick) return;
          const chartWidth = (e.currentTarget as HTMLElement).clientWidth;
          const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
          const x = e.clientX - rect.left;
          const margin = 40;
          const plotWidth = chartWidth - margin - 20;
          const round = Math.round(((x - margin) / plotWidth) * 63);
          if (round >= 0 && round <= 63) {
            onRoundClick(round);
          }
        }}
      >
        <ComposedChart
          data={chartData}
          margin={{ top: 5, right: 10, left: 0, bottom: 5 }}
        >
          <defs>
            <linearGradient id="avalancheZone" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="#f97316" stopOpacity={0.15} />
              <stop offset="100%" stopColor="#f97316" stopOpacity={0.02} />
            </linearGradient>
            <linearGradient id="diffusionGrad" x1="0" y1="0" x2="1" y2="0">
              <stop offset="0%" stopColor="#10b981" />
              <stop offset="50%" stopColor="#06b6d4" />
              <stop offset="100%" stopColor="#f97316" />
            </linearGradient>
          </defs>
          <CartesianGrid
            strokeDasharray="3 3"
            stroke="#27272a"
            vertical={false}
          />
          <XAxis
            dataKey="round"
            tick={{ fontSize: 9, fill: "#71717a" }}
            tickCount={8}
            stroke="#27272a"
          />
          <YAxis
            tick={{ fontSize: 9, fill: "#71717a" }}
            domain={[0, 100]}
            tickCount={5}
            stroke="#27272a"
            tickFormatter={(v) => `${v}%`}
          />
          {/* Avalanche zone (rounds 16-24) */}
          <ReferenceLine
            x={16}
            stroke="#f9731640"
            strokeDasharray="3 3"
          />
          <ReferenceLine
            x={24}
            stroke="#f9731640"
            strokeDasharray="3 3"
          />
          {/* 50% reference line */}
          <ReferenceLine
            y={50}
            stroke="#f97316"
            strokeDasharray="5 5"
            strokeWidth={1}
            label={{
              value: "50%",
              position: "right",
              fill: "#f97316",
              fontSize: 9,
            }}
          />
          {/* Current round indicator */}
          <ReferenceLine
            x={currentRound}
            stroke="#06b6d4"
            strokeWidth={2}
            strokeDasharray="2 2"
          />
          <Area
            type="monotone"
            dataKey="diffusion"
            fill="url(#avalancheZone)"
            stroke="none"
          />
          <Line
            type="monotone"
            dataKey="diffusion"
            stroke="url(#diffusionGrad)"
            strokeWidth={2}
            dot={false}
            activeDot={{
              r: 4,
              fill: "#06b6d4",
              stroke: "#06b6d4",
              strokeWidth: 2,
            }}
          />
        </ComposedChart>
      </div>
      <div className="flex justify-between text-[9px] text-zinc-600 px-8">
        <span>Avalanche zone: R16-R24</span>
        <span>Click chart to jump to round</span>
      </div>
    </div>
  );
}
