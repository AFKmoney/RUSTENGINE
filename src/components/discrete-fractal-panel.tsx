'use client';

import React, { useState, useMemo } from "react";
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
} from "recharts";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Slider } from "@/components/ui/slider";
import type {
  FullDiscreteAnalysis,
  DiscreteBoxCountingResult,
  WalshSpectrumResult,
  SelfSimilarityResult,
  ClusterTreeResult,
  DimensionProfile,
} from "@/lib/discrete-fractal";

interface DiscreteFractalPanelProps {
  analysis: FullDiscreteAnalysis;
  currentRound: number;
  onRoundChange?: (round: number) => void;
}

// Color helpers
function dimensionToColor(dim: number): string {
  if (dim < 100) return "#ef4444"; // red - severe anomaly
  if (dim < 200) return "#f97316"; // orange - anomaly
  if (dim < 240) return "#eab308"; // yellow - borderline
  return "#10b981"; // emerald - normal
}

function correlationToColor(corr: number): string {
  const intensity = Math.min(corr * 2, 1);
  const r = Math.round(16 + intensity * 230);
  const g = Math.round(185 - intensity * 100);
  const b = Math.round(129 - intensity * 80);
  return `rgb(${r},${g},${b})`;
}

// --- Tab 1: Dimension Profile ---

function DimensionProfileTab({
  analysis,
  currentRound,
}: {
  analysis: FullDiscreteAnalysis;
  currentRound: number;
}) {
  const [selectedRounds, setSelectedRounds] = useState<number[]>([0, 4, 8, 16, 32, 48, 63]);

  // Build chart data: x = scale (log), multiple lines for selected rounds
  const scales = analysis.boxCounting[0]?.scales || [];
  const chartData = scales.map((scale, si) => {
    const point: Record<string, number> = { scale: Math.log2(scale) };
    for (const r of selectedRounds) {
      if (analysis.dimensionProfile[r]) {
        const profile = analysis.dimensionProfile[r].profile;
        // Find the dimension at this scale
        const entry = profile.find((p) => p[0] === scale);
        point[`R${r}`] = entry ? entry[1] : 256;
      }
    }
    return point;
  });

  // Color gradient green→orange based on round index
  const roundColors = [
    "#10b981", "#34d399", "#6ee7b7", "#a7f3d0", // emerald shades
    "#06b6d4", "#22d3ee", "#67e8f9", // cyan shades
    "#f59e0b", "#fbbf24", "#fcd34d", // amber shades
    "#f97316", // orange
  ];

  const lines = selectedRounds.map((r, i) => (
    <Line
      key={r}
      type="monotone"
      dataKey={`R${r}`}
      stroke={roundColors[i % roundColors.length]}
      strokeWidth={r === currentRound ? 3 : 1.5}
      dot={false}
      connectNulls
    />
  ));

  // Find rounds with anomalies
  const anomalyRounds = analysis.dimensionProfile
    .filter((dp) => dp.isAnomaly)
    .map((dp) => dp.round);

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-[10px] text-zinc-500 uppercase">Selected Rounds:</span>
        {selectedRounds.map((r) => (
          <Badge
            key={r}
            variant={r === currentRound ? "default" : "outline"}
            className={`text-[9px] font-mono cursor-pointer ${
              anomalyRounds.includes(r)
                ? "border-red-500/50 text-red-400"
                : r === currentRound
                  ? "bg-emerald-600 text-white"
                  : "border-zinc-700 text-zinc-400"
            }`}
            onClick={() => {
              setSelectedRounds((prev) =>
                prev.includes(r) && prev.length > 1
                  ? prev.filter((x) => x !== r)
                  : prev
              );
            }}
          >
            R{r}
            {anomalyRounds.includes(r) && " ⚠"}
          </Badge>
        ))}
      </div>

      <div className="h-72">
        <ResponsiveContainer width="100%" height="100%">
          <LineChart data={chartData}>
            <CartesianGrid strokeDasharray="3 3" stroke="#27272a" />
            <XAxis
              dataKey="scale"
              tick={{ fill: "#71717a", fontSize: 10 }}
              label={{ value: "log₂(scale)", position: "insideBottom", offset: -2, style: { fill: "#71717a", fontSize: 9 } }}
            />
            <YAxis
              domain={[0, 300]}
              tick={{ fill: "#71717a", fontSize: 10 }}
              label={{ value: "Dimension", angle: -90, position: "insideLeft", style: { fill: "#71717a", fontSize: 9 } }}
            />
            <Tooltip
              contentStyle={{ backgroundColor: "#18181b", border: "1px solid #3f3f46", borderRadius: 8, fontSize: 10 }}
              labelStyle={{ color: "#a1a1aa" }}
            />
            <ReferenceLine y={256} stroke="#10b981" strokeDasharray="5 5" label={{ value: "D=256", position: "right", style: { fill: "#10b981", fontSize: 9 } }} />
            <ReferenceLine y={200} stroke="#f97316" strokeDasharray="5 5" label={{ value: "D=200", position: "right", style: { fill: "#f97316", fontSize: 9 } }} />
            {lines}
          </LineChart>
        </ResponsiveContainer>
      </div>

      {/* Summary table */}
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-2">
        {selectedRounds.map((r) => {
          const dp = analysis.dimensionProfile[r];
          return (
            <div
              key={r}
              className={`rounded-lg p-2 text-center border ${
                dp?.isAnomaly ? "border-red-500/30 bg-red-950/20" : "border-zinc-800 bg-zinc-900/60"
              }`}
            >
              <div className="text-[9px] text-zinc-500 font-mono">Round {r}</div>
              <div className="text-sm font-mono font-bold" style={{ color: dimensionToColor(dp?.minDimension ?? 256) }}>
                {dp?.minDimension.toFixed(1) ?? "—"}
              </div>
              <div className="text-[8px] text-zinc-600">min D</div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

// --- Tab 2: Influence Map (Walsh) ---

function InfluenceMapTab({
  analysis,
  currentRound,
}: {
  analysis: FullDiscreteAnalysis;
  currentRound: number;
}) {
  const [hoveredCell, setHoveredCell] = useState<{
    inputByte: number;
    outputWord: number;
    corr: number;
  } | null>(null);

  const ws = analysis.walshSpectrum[currentRound];
  if (!ws) return null;

  // Group correlations: 256 output bits → 8 words × 32 bits
  // For display, group input bits into 32 bytes, output bits into 8 words
  // Each cell = average |correlation| for that (inputByte, outputWord) pair
  const inputBytes = 32;
  const outputWords = 8;

  // Build grid data
  const gridData: number[][] = [];
  for (let ib = 0; ib < inputBytes; ib++) {
    const row: number[] = [];
    for (let ow = 0; ow < outputWords; ow++) {
      // Average correlation for input bits [ib*8..ib*8+7] → output word ow
      let sum = 0;
      for (let bit = 0; bit < 8; bit++) {
        const outputBit = ow * 32 + bit;
        // Average across all 8 bits in the input byte
        for (let inputBit = ib * 8; inputBit < ib * 8 + 8; inputBit++) {
          const idx = ow * 32 + (inputBit % 32);
          if (idx < ws.avgAbsCorrelation.length) {
            sum += ws.avgAbsCorrelation[idx];
          }
        }
      }
      row.push(sum / 8);
    }
    gridData.push(row);
  }

  // Compute max for normalization
  const maxCorr = Math.max(...gridData.flat(), 0.01);

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-3 flex-wrap">
        <Badge variant="outline" className="text-[9px] border-cyan-600/50 text-cyan-400">
          Spectral Flatness: {ws.spectralFlatness.toFixed(4)}
        </Badge>
        <Badge
          variant="outline"
          className={`text-[9px] ${
            ws.spectralFlatness < 0.95 ? "border-orange-500/50 text-orange-400" : "border-emerald-500/50 text-emerald-400"
          }`}
        >
          {ws.spectralFlatness < 0.95 ? "⚠ Structured" : "✓ Random-like"}
        </Badge>
        <Badge variant="outline" className="text-[9px] border-zinc-600 text-zinc-400">
          Max Corr: {ws.maxCorrelation.toFixed(3)}
        </Badge>
      </div>

      {/* Heatmap grid */}
      <div className="relative">
        <div className="flex gap-1">
          {/* Y-axis label */}
          <div className="flex flex-col justify-between pr-1 py-0.5">
            <span className="text-[7px] text-zinc-600 font-mono">31</span>
            <span className="text-[7px] text-zinc-600 font-mono">16</span>
            <span className="text-[7px] text-zinc-600 font-mono">0</span>
          </div>

          <div>
            {/* X-axis labels */}
            <div className="flex gap-px mb-1 pl-0.5">
              {Array.from({ length: outputWords }).map((_, i) => (
                <div key={i} className="w-10 text-center text-[7px] text-zinc-600 font-mono">
                  {["a", "b", "c", "d", "e", "f", "g", "h"][i]}
                </div>
              ))}
            </div>

            {/* Grid cells */}
            <div className="flex flex-col gap-px">
              {gridData.map((row, ib) => (
                <div key={ib} className="flex gap-px">
                  {row.map((corr, ow) => {
                    const intensity = corr / maxCorr;
                    const isAnomaly = corr > 0.3;
                    return (
                      <div
                        key={ow}
                        className={`w-10 h-3 rounded-sm cursor-pointer transition-all ${
                          isAnomaly ? "ring-1 ring-red-500/50" : ""
                        }`}
                        style={{
                          backgroundColor: isAnomaly
                            ? `rgba(239, 68, 68, ${0.3 + intensity * 0.7})`
                            : `rgba(6, 182, 212, ${intensity * 0.8})`,
                        }}
                        onMouseEnter={() =>
                          setHoveredCell({ inputByte: ib, outputWord: ow, corr })
                        }
                        onMouseLeave={() => setHoveredCell(null)}
                      />
                    );
                  })}
                </div>
              ))}
            </div>
          </div>
        </div>

        {/* Tooltip */}
        {hoveredCell && (
          <div className="absolute top-0 right-0 bg-zinc-800 border border-zinc-700 rounded-lg p-2 text-[9px] font-mono z-10">
            <div className="text-zinc-400">
              Input Byte {hoveredCell.inputByte} → Word {["a", "b", "c", "d", "e", "f", "g", "h"][hoveredCell.outputWord]}
            </div>
            <div className="text-cyan-400">Correlation: {hoveredCell.corr.toFixed(4)}</div>
            {hoveredCell.corr > 0.3 && (
              <div className="text-red-400">⚠ Anomalous</div>
            )}
          </div>
        )}
      </div>

      {/* Per-output-bit correlation bars (simplified: show 8 word averages) */}
      <div className="space-y-1">
        <div className="text-[9px] text-zinc-500 uppercase">Per-Word Average Correlation</div>
        <div className="flex gap-1">
          {Array.from({ length: 8 }).map((_, w) => {
            let sum = 0;
            for (let b = 0; b < 32; b++) {
              sum += ws.avgAbsCorrelation[w * 32 + b];
            }
            const avg = sum / 32;
            const height = Math.min(avg * 200, 100);
            return (
              <div key={w} className="flex-1 flex flex-col items-center gap-0.5">
                <div className="w-full h-16 bg-zinc-800/50 rounded-sm relative overflow-hidden">
                  <div
                    className="absolute bottom-0 left-0 right-0 rounded-sm transition-all"
                    style={{
                      height: `${height}%`,
                      backgroundColor: avg > 0.3 ? "#f97316" : avg > 0.15 ? "#06b6d4" : "#10b981",
                    }}
                  />
                </div>
                <span className="text-[7px] font-mono text-zinc-600">
                  {["a", "b", "c", "d", "e", "f", "g", "h"][w]}
                </span>
                <span className="text-[7px] font-mono text-zinc-500">{avg.toFixed(3)}</span>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

// --- Tab 3: Self-Similarity ---

function SelfSimilarityTab({
  analysis,
  currentRound,
}: {
  analysis: FullDiscreteAnalysis;
  currentRound: number;
}) {
  const ss = analysis.selfSimilarity[currentRound];
  if (!ss) return null;

  // Build chart data for distance histogram
  const histogramData = ss.distanceHistogram
    .map((count, dist) => ({
      distance: dist,
      count,
      // Normalized for overlay comparison
      normalized: count / Math.max(...ss.distanceHistogram, 1),
    }))
    .filter((d) => d.count > 0)
    .slice(0, 200); // limit display range

  // Build scale divergence data
  const scaleLabels = ["1→2", "2→4", "4→8", "8→16", "16→32"];
  const divergenceData = ss.scaleDivergences.map((d, i) => ({
    scale: scaleLabels[i] || `${i}→${i + 1}`,
    divergence: d,
    isAnomaly: d > 5,
  }));

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-3 flex-wrap">
        <Badge
          variant="outline"
          className={`text-[9px] ${
            ss.isAnomalous ? "border-red-500/50 text-red-400" : "border-emerald-500/50 text-emerald-400"
          }`}
        >
          Self-Similarity: {ss.selfSimilarityScore.toFixed(4)}
        </Badge>
        <Badge
          variant="outline"
          className={`text-[9px] ${
            ss.isAnomalous ? "border-orange-500/50 text-orange-400" : "border-cyan-500/50 text-cyan-400"
          }`}
        >
          {ss.isAnomalous ? "⚠ Anomalous" : "✓ Self-Similar"}
        </Badge>
      </div>

      {/* Distance Histogram */}
      <div>
        <div className="text-[9px] text-zinc-500 uppercase mb-1">Hamming Distance Distribution</div>
        <div className="h-48">
          <ResponsiveContainer width="100%" height="100%">
            <BarChart data={histogramData}>
              <CartesianGrid strokeDasharray="3 3" stroke="#27272a" />
              <XAxis
                dataKey="distance"
                tick={{ fill: "#71717a", fontSize: 9 }}
                label={{ value: "Hamming Distance", position: "insideBottom", offset: -2, style: { fill: "#71717a", fontSize: 9 } }}
              />
              <YAxis
                tick={{ fill: "#71717a", fontSize: 9 }}
                label={{ value: "Count", angle: -90, position: "insideLeft", style: { fill: "#71717a", fontSize: 9 } }}
              />
              <Tooltip
                contentStyle={{ backgroundColor: "#18181b", border: "1px solid #3f3f46", borderRadius: 8, fontSize: 10 }}
              />
              <Bar dataKey="count" radius={[2, 2, 0, 0]}>
                {histogramData.map((entry, index) => (
                  <Cell
                    key={index}
                    fill={entry.normalized > 0.8 ? "#f97316" : "#06b6d4"}
                    fillOpacity={0.5 + entry.normalized * 0.5}
                  />
                ))}
              </Bar>
            </BarChart>
          </ResponsiveContainer>
        </div>
      </div>

      {/* Scale Divergences */}
      <div>
        <div className="text-[9px] text-zinc-500 uppercase mb-1">KL Divergences Across Scales</div>
        <div className="h-36">
          <ResponsiveContainer width="100%" height="100%">
            <BarChart data={divergenceData}>
              <CartesianGrid strokeDasharray="3 3" stroke="#27272a" />
              <XAxis dataKey="scale" tick={{ fill: "#71717a", fontSize: 9 }} />
              <YAxis tick={{ fill: "#71717a", fontSize: 9 }} />
              <Tooltip
                contentStyle={{ backgroundColor: "#18181b", border: "1px solid #3f3f46", borderRadius: 8, fontSize: 10 }}
              />
              <ReferenceLine y={5} stroke="#f97316" strokeDasharray="5 5" label={{ value: "Threshold", position: "right", style: { fill: "#f97316", fontSize: 9 } }} />
              <Bar dataKey="divergence" radius={[4, 4, 0, 0]}>
                {divergenceData.map((entry, index) => (
                  <Cell
                    key={index}
                    fill={entry.isAnomaly ? "#ef4444" : "#10b981"}
                  />
                ))}
              </Bar>
            </BarChart>
          </ResponsiveContainer>
        </div>
      </div>
    </div>
  );
}

// --- Tab 4: Resonance Scanner ---

function ResonanceScannerTab({
  analysis,
  currentRound,
  onRoundChange,
}: {
  analysis: FullDiscreteAnalysis;
  currentRound: number;
  onRoundChange?: (round: number) => void;
}) {
  // Build 2D heatmap: rounds (0-63) × scales
  // Color = dimension at that (round, scale) pair
  const scales = analysis.boxCounting[0]?.scales.slice(0, -1) || []; // exclude last (no dim estimate)
  const numScales = scales.length;

  // Compute color for each cell
  const getCellColor = (round: number, scaleIndex: number): string => {
    const bc = analysis.boxCounting[round];
    if (!bc || scaleIndex >= bc.dimensionEstimates.length) return "#18181b";
    const dim = bc.dimensionEstimates[scaleIndex];
    return dimensionToColor(dim);
  };

  const getCellGlow = (round: number, scaleIndex: number): string => {
    const bc = analysis.boxCounting[round];
    if (!bc || scaleIndex >= bc.dimensionEstimates.length) return "";
    const dim = bc.dimensionEstimates[scaleIndex];
    if (dim < 200) return "shadow-[0_0_6px_rgba(239,68,68,0.6)]";
    if (dim < 240) return "shadow-[0_0_4px_rgba(249,115,22,0.4)]";
    return "";
  };

  // Count anomalies
  const anomalyCount = analysis.boxCounting.filter((bc) => bc.hasAnomaly).length;
  const totalCells = 64 * numScales;

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-3 flex-wrap">
        <Badge variant="outline" className="text-[9px] border-zinc-600 text-zinc-400">
          {totalCells} cells scanned
        </Badge>
        {anomalyCount > 0 && (
          <Badge variant="outline" className="text-[9px] border-red-500/50 text-red-400">
            ⚠ {anomalyCount} rounds with anomalies
          </Badge>
        )}
        {anomalyCount === 0 && (
          <Badge variant="outline" className="text-[9px] border-emerald-500/50 text-emerald-400">
            ✓ No anomalies detected
          </Badge>
        )}
      </div>

      {/* Legend */}
      <div className="flex items-center gap-4 text-[8px] font-mono text-zinc-500">
        <div className="flex items-center gap-1">
          <div className="w-3 h-3 rounded-sm bg-[#10b981]" />
          <span>D ≈ 256 (Random)</span>
        </div>
        <div className="flex items-center gap-1">
          <div className="w-3 h-3 rounded-sm bg-[#eab308]" />
          <span>D &lt; 240 (Borderline)</span>
        </div>
        <div className="flex items-center gap-1">
          <div className="w-3 h-3 rounded-sm bg-[#f97316]" />
          <span>D &lt; 200 (Anomaly)</span>
        </div>
        <div className="flex items-center gap-1">
          <div className="w-3 h-3 rounded-sm bg-[#ef4444]" />
          <span>D &lt; 100 (Severe)</span>
        </div>
      </div>

      {/* Heatmap */}
      <div className="overflow-x-auto">
        <div className="inline-block">
          {/* Scale labels */}
          <div className="flex gap-px mb-1 pl-8">
            {scales.map((s, i) => (
              <div key={i} className="w-4 text-center text-[7px] text-zinc-600 font-mono">
                {s}
              </div>
            ))}
          </div>

          {/* Rows: one per round */}
          <div className="flex flex-col gap-px">
            {Array.from({ length: 64 }).map((_, round) => (
              <div key={round} className="flex items-center gap-px">
                <div
                  className={`w-7 text-right text-[7px] font-mono pr-1 cursor-pointer ${
                    round === currentRound ? "text-emerald-400 font-bold" : "text-zinc-600"
                  }`}
                  onClick={() => onRoundChange?.(round)}
                >
                  {round}
                </div>
                {Array.from({ length: numScales }).map((_, si) => (
                  <div
                    key={si}
                    className={`w-4 h-3 rounded-[1px] cursor-pointer transition-transform hover:scale-150 hover:z-10 ${getCellGlow(round, si)}`}
                    style={{ backgroundColor: getCellColor(round, si) }}
                    title={`R${round} s=${scales[si]} D=${analysis.boxCounting[round]?.dimensionEstimates[si]?.toFixed(1) ?? "—"}`}
                    onClick={() => onRoundChange?.(round)}
                  />
                ))}
              </div>
            ))}
          </div>
        </div>
      </div>

      {/* Per-round dimension summary */}
      <div className="h-32">
        <ResponsiveContainer width="100%" height="100%">
          <LineChart
            data={analysis.dimensionProfile.map((dp) => ({
              round: dp.round,
              minDim: dp.minDimension,
              isAnomaly: dp.isAnomaly ? 200 : null,
            }))}
          >
            <CartesianGrid strokeDasharray="3 3" stroke="#27272a" />
            <XAxis dataKey="round" tick={{ fill: "#71717a", fontSize: 9 }} />
            <YAxis domain={[0, 300]} tick={{ fill: "#71717a", fontSize: 9 }} />
            <Tooltip
              contentStyle={{ backgroundColor: "#18181b", border: "1px solid #3f3f46", borderRadius: 8, fontSize: 10 }}
            />
            <ReferenceLine y={256} stroke="#10b981" strokeDasharray="3 3" strokeOpacity={0.5} />
            <ReferenceLine y={200} stroke="#f97316" strokeDasharray="3 3" strokeOpacity={0.5} />
            <Line type="monotone" dataKey="minDim" stroke="#06b6d4" strokeWidth={1.5} dot={false} />
          </LineChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}

// --- Tab 5: Clusters ---

function ClustersTab({
  analysis,
  currentRound,
}: {
  analysis: FullDiscreteAnalysis;
  currentRound: number;
}) {
  const ct = analysis.clusterTree[currentRound];
  if (!ct) return null;

  const thresholds = [8, 16, 32, 64, 128];
  const clusterData = ct.clusterCounts.map((count, i) => ({
    threshold: `d ≤ ${thresholds[i]}`,
    clusters: count,
    isAnomaly: count <= 1,
  }));

  // Compute overview across all rounds
  const allImbalances = analysis.clusterTree.map((ct) => ct.imbalance);
  const allMaxFractions = analysis.clusterTree.map((ct) => ct.maxClusterFraction);

  const imbalanceData = analysis.clusterTree.map((ct) => ({
    round: ct.round,
    imbalance: ct.imbalance,
    maxFraction: ct.maxClusterFraction,
    isAnomaly: ct.isAnomalous,
  }));

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-3 flex-wrap">
        <Badge
          variant="outline"
          className={`text-[9px] ${
            ct.isAnomalous ? "border-red-500/50 text-red-400" : "border-emerald-500/50 text-emerald-400"
          }`}
        >
          {ct.isAnomalous ? "⚠ Imbalanced Clusters" : "✓ Balanced Clusters"}
        </Badge>
        <Badge variant="outline" className="text-[9px] border-zinc-600 text-zinc-400">
          Imbalance: {ct.imbalance.toFixed(3)}
        </Badge>
        <Badge variant="outline" className="text-[9px] border-zinc-600 text-zinc-400">
          Max Cluster: {(ct.maxClusterFraction * 100).toFixed(1)}%
        </Badge>
      </div>

      {/* Cluster counts at current round */}
      <div>
        <div className="text-[9px] text-zinc-500 uppercase mb-1">
          Cluster Count by Distance Threshold — Round {currentRound}
        </div>
        <div className="h-40">
          <ResponsiveContainer width="100%" height="100%">
            <BarChart data={clusterData}>
              <CartesianGrid strokeDasharray="3 3" stroke="#27272a" />
              <XAxis dataKey="threshold" tick={{ fill: "#71717a", fontSize: 9 }} />
              <YAxis tick={{ fill: "#71717a", fontSize: 9 }} />
              <Tooltip
                contentStyle={{ backgroundColor: "#18181b", border: "1px solid #3f3f46", borderRadius: 8, fontSize: 10 }}
              />
              <Bar dataKey="clusters" radius={[4, 4, 0, 0]}>
                {clusterData.map((entry, index) => (
                  <Cell
                    key={index}
                    fill={entry.isAnomaly ? "#ef4444" : "#10b981"}
                  />
                ))}
              </Bar>
            </BarChart>
          </ResponsiveContainer>
        </div>
      </div>

      {/* Imbalance across all rounds */}
      <div>
        <div className="text-[9px] text-zinc-500 uppercase mb-1">Cluster Imbalance Across Rounds</div>
        <div className="h-36">
          <ResponsiveContainer width="100%" height="100%">
            <LineChart data={imbalanceData}>
              <CartesianGrid strokeDasharray="3 3" stroke="#27272a" />
              <XAxis dataKey="round" tick={{ fill: "#71717a", fontSize: 9 }} />
              <YAxis domain={[0, 1]} tick={{ fill: "#71717a", fontSize: 9 }} />
              <Tooltip
                contentStyle={{ backgroundColor: "#18181b", border: "1px solid #3f3f46", borderRadius: 8, fontSize: 10 }}
              />
              <ReferenceLine y={0.8} stroke="#f97316" strokeDasharray="3 3" strokeOpacity={0.5} />
              <Line type="monotone" dataKey="imbalance" stroke="#f97316" strokeWidth={1.5} dot={false} />
              <Line type="monotone" dataKey="maxFraction" stroke="#06b6d4" strokeWidth={1} dot={false} strokeDasharray="3 3" />
              <Legend
                wrapperStyle={{ fontSize: 9, color: "#71717a" }}
              />
            </LineChart>
          </ResponsiveContainer>
        </div>
      </div>
    </div>
  );
}

// --- Main Component ---

export default function DiscreteFractalPanel({
  analysis,
  currentRound,
  onRoundChange,
}: DiscreteFractalPanelProps) {
  const [activeTab, setActiveTab] = useState("resonance");

  // Compute overall summary
  const summary = useMemo(() => {
    const anomalyRounds = analysis.dimensionProfile.filter((dp) => dp.isAnomaly).length;
    const avgFlatness =
      analysis.walshSpectrum.reduce((s, ws) => s + ws.spectralFlatness, 0) / 64;
    const avgSelfSim =
      analysis.selfSimilarity.reduce((s, ss) => s + ss.selfSimilarityScore, 0) / 64;
    const avgMinDim =
      analysis.dimensionProfile.reduce((s, dp) => s + dp.minDimension, 0) / 64;

    return {
      anomalyRounds,
      avgFlatness,
      avgSelfSim,
      avgMinDim,
    };
  }, [analysis]);

  return (
    <div className="space-y-4">
      {/* Summary Bar */}
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-2">
        <div className="bg-zinc-900/80 border border-zinc-800 rounded-lg p-3 text-center">
          <div className="text-[9px] text-zinc-500 uppercase">Anomaly Rounds</div>
          <div className={`text-lg font-mono font-bold ${summary.anomalyRounds > 0 ? "text-red-400" : "text-emerald-400"}`}>
            {summary.anomalyRounds}
            <span className="text-xs text-zinc-600">/64</span>
          </div>
        </div>
        <div className="bg-zinc-900/80 border border-zinc-800 rounded-lg p-3 text-center">
          <div className="text-[9px] text-zinc-500 uppercase">Avg Dimension</div>
          <div className="text-lg font-mono font-bold text-cyan-400">
            {summary.avgMinDim.toFixed(1)}
          </div>
        </div>
        <div className="bg-zinc-900/80 border border-zinc-800 rounded-lg p-3 text-center">
          <div className="text-[9px] text-zinc-500 uppercase">Spectral Flatness</div>
          <div className={`text-lg font-mono font-bold ${summary.avgFlatness < 0.95 ? "text-orange-400" : "text-emerald-400"}`}>
            {summary.avgFlatness.toFixed(3)}
          </div>
        </div>
        <div className="bg-zinc-900/80 border border-zinc-800 rounded-lg p-3 text-center">
          <div className="text-[9px] text-zinc-500 uppercase">Self-Similarity</div>
          <div className={`text-lg font-mono font-bold ${summary.avgSelfSim < 0.3 ? "text-red-400" : "text-emerald-400"}`}>
            {summary.avgSelfSim.toFixed(3)}
          </div>
        </div>
      </div>

      {/* Tabs */}
      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList className="bg-zinc-900/80 border border-zinc-800 h-8">
          <TabsTrigger value="profile" className="text-[10px] px-2">Dimension</TabsTrigger>
          <TabsTrigger value="influence" className="text-[10px] px-2">Influence</TabsTrigger>
          <TabsTrigger value="similarity" className="text-[10px] px-2">Self-Sim</TabsTrigger>
          <TabsTrigger value="resonance" className="text-[10px] px-2">Resonance</TabsTrigger>
          <TabsTrigger value="clusters" className="text-[10px] px-2">Clusters</TabsTrigger>
        </TabsList>

        <TabsContent value="profile">
          <Card className="bg-zinc-900/60 border-zinc-800">
            <CardHeader className="pb-2 pt-3 px-4">
              <CardTitle className="text-xs font-semibold text-zinc-400 uppercase tracking-wider">
                Discrete Dimension Profile
              </CardTitle>
            </CardHeader>
            <CardContent className="px-4 pb-4">
              <DimensionProfileTab analysis={analysis} currentRound={currentRound} />
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="influence">
          <Card className="bg-zinc-900/60 border-zinc-800">
            <CardHeader className="pb-2 pt-3 px-4">
              <CardTitle className="text-xs font-semibold text-zinc-400 uppercase tracking-wider">
                Walsh-Hadamard Influence Map
              </CardTitle>
            </CardHeader>
            <CardContent className="px-4 pb-4">
              <InfluenceMapTab analysis={analysis} currentRound={currentRound} />
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="similarity">
          <Card className="bg-zinc-900/60 border-zinc-800">
            <CardHeader className="pb-2 pt-3 px-4">
              <CardTitle className="text-xs font-semibold text-zinc-400 uppercase tracking-wider">
                Discrete Self-Similarity Analysis
              </CardTitle>
            </CardHeader>
            <CardContent className="px-4 pb-4">
              <SelfSimilarityTab analysis={analysis} currentRound={currentRound} />
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="resonance">
          <Card className="bg-zinc-900/60 border-zinc-800">
            <CardHeader className="pb-2 pt-3 px-4">
              <CardTitle className="text-xs font-semibold text-zinc-400 uppercase tracking-wider">
                Resonance Scanner — Dimension at (Round × Scale)
              </CardTitle>
            </CardHeader>
            <CardContent className="px-4 pb-4">
              <ResonanceScannerTab
                analysis={analysis}
                currentRound={currentRound}
                onRoundChange={onRoundChange}
              />
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="clusters">
          <Card className="bg-zinc-900/60 border-zinc-800">
            <CardHeader className="pb-2 pt-3 px-4">
              <CardTitle className="text-xs font-semibold text-zinc-400 uppercase tracking-wider">
                Cluster Tree Analysis
              </CardTitle>
            </CardHeader>
            <CardContent className="px-4 pb-4">
              <ClustersTab analysis={analysis} currentRound={currentRound} />
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  );
}
