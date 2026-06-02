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
import DiscreteFractalPanel from "@/components/discrete-fractal-panel";
import {
  PUZZLES,
  getPuzzleByNumber,
  getUnsolvedPuzzles,
  getPuzzlesWithPublicKey,
  formatRange,
  getRangeSize,
  estimateKangarooTimeSec,
} from "@/lib/puzzle-db";
import type { BitcoinPuzzle } from "@/lib/puzzle-db";
import {
  pollardKangaroo,
  bruteForceSearch,
  parseCompressedPubkey,
  estimateKangarooIterations,
  estimateTimeSeconds,
  formatTime,
  type KangarooProgress,
  type KangarooResult,
} from "@/lib/kangaroo";
import {
  pubkeyToAddress,
  verifyPrivateKey,
  verifyPrivateKeyAgainstPubkey,
  computeFullPipeline,
} from "@/lib/bitcoin-address";
import { validatePublicKey } from "@/lib/secp256k1";
import { pubkeyToSha256Block, input33ToSha256Block, generateRandomInput33 } from "@/lib/bitcoin-pipeline";
import { computeFullDiscreteAnalysis } from "@/lib/discrete-fractal";
import type { FullDiscreteAnalysis } from "@/lib/discrete-fractal";
import {
  Crosshair,
  Play,
  Square,
  Copy,
  Check,
  AlertTriangle,
  Zap,
  Search,
  Key,
  Target,
  Activity,
  Cpu,
  Timer,
  Hash,
  ChevronDown,
  ChevronRight,
} from "lucide-react";

// --- Section A: Puzzle Selector ---

type PuzzleFilter = "all" | "unsolved" | "with_pubkey";

function PuzzleSelector({
  selectedPuzzle,
  onSelectPuzzle,
}: {
  selectedPuzzle: BitcoinPuzzle | null;
  onSelectPuzzle: (puzzle: BitcoinPuzzle) => void;
}) {
  const [filter, setFilter] = useState<PuzzleFilter>("unsolved");
  const [searchTerm, setSearchTerm] = useState("");

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

  const getStatusColor = (puzzle: BitcoinPuzzle) => {
    if (puzzle.solved) return "text-emerald-400";
    if (puzzle.publicKeyCompressed) return "text-orange-400";
    return "text-red-400";
  };

  const getStatusBg = (puzzle: BitcoinPuzzle) => {
    if (puzzle.solved) return "bg-emerald-950/30 border-emerald-500/20";
    if (puzzle.publicKeyCompressed) return "bg-orange-950/30 border-orange-500/20";
    return "bg-red-950/30 border-red-500/20";
  };

  const getStatusLabel = (puzzle: BitcoinPuzzle) => {
    if (puzzle.solved) return "SOLVED";
    if (puzzle.publicKeyCompressed) return "PUBKEY KNOWN";
    return "UNSOLVED";
  };

  return (
    <Card className="bg-zinc-900/60 border-zinc-800">
      <CardHeader className="pb-2 pt-3 px-4">
        <CardTitle className="text-xs font-semibold text-zinc-400 uppercase tracking-wider flex items-center gap-2">
          <Target className="h-3.5 w-3.5 text-red-400" />
          Puzzle Selector
          <Badge variant="outline" className="text-[8px] border-zinc-700 text-zinc-500 ml-auto">
            {PUZZLES.filter(p => !p.solved).length} unsolved / {PUZZLES.length} total
          </Badge>
        </CardTitle>
      </CardHeader>
      <CardContent className="px-4 pb-4 space-y-3">
        {/* Filters */}
        <div className="flex items-center gap-2 flex-wrap">
          {(["all", "unsolved", "with_pubkey"] as PuzzleFilter[]).map((f) => (
            <Button
              key={f}
              size="sm"
              variant={filter === f ? "default" : "outline"}
              onClick={() => setFilter(f)}
              className={`h-7 text-[10px] ${
                filter === f
                  ? f === "unsolved"
                    ? "bg-red-600 hover:bg-red-500 text-white"
                    : f === "with_pubkey"
                    ? "bg-orange-600 hover:bg-orange-500 text-white"
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
            className="font-mono text-[10px] bg-zinc-950 border-zinc-700 text-zinc-300 h-7 flex-1 min-w-[180px]"
          />
        </div>

        {/* Puzzle list */}
        <div className="max-h-72 overflow-y-auto rounded-lg border border-zinc-800 bg-zinc-950/40">
          <div className="sticky top-0 bg-zinc-900/95 backdrop-blur-sm border-b border-zinc-800 grid grid-cols-[40px_1fr_80px_70px_60px] gap-1 px-2 py-1.5 text-[8px] text-zinc-500 uppercase font-semibold z-10">
            <span>#</span>
            <span>Address / Pubkey</span>
            <span>Range</span>
            <span>Status</span>
            <span>BTC</span>
          </div>
          {filteredPuzzles.map((puzzle) => (
            <div
              key={puzzle.number}
              className={`grid grid-cols-[40px_1fr_80px_70px_60px] gap-1 px-2 py-1.5 border-b border-zinc-800/50 cursor-pointer hover:bg-zinc-800/40 transition-colors text-[9px] font-mono ${
                selectedPuzzle?.number === puzzle.number ? "bg-zinc-800/60" : ""
              }`}
              onClick={() => onSelectPuzzle(puzzle)}
            >
              <span className="text-zinc-300 font-bold">{puzzle.number}</span>
              <span className="text-zinc-500 truncate">
                {puzzle.address || (puzzle.publicKeyCompressed ? puzzle.publicKeyCompressed.slice(0, 20) + "..." : "—")}
              </span>
              <span className="text-zinc-600">{formatRange(puzzle.rangeStart, puzzle.rangeEnd)}</span>
              <span>
                <Badge
                  variant="outline"
                  className={`text-[7px] h-4 px-1 ${getStatusBg(puzzle)} ${getStatusColor(puzzle)}`}
                >
                  {getStatusLabel(puzzle)}
                </Badge>
              </span>
              <span className="text-zinc-500">{puzzle.balance?.toFixed(1) || "—"}</span>
            </div>
          ))}
        </div>
      </CardContent>
    </Card>
  );
}

// --- Section B: Target Input ---

function TargetInputSection({
  puzzle,
  onPubkeyChange,
}: {
  puzzle: BitcoinPuzzle | null;
  onPubkeyChange: (pubkeyHex: string | null, point: { x: bigint; y: bigint } | null) => void;
}) {
  const [manualPubkey, setManualPubkey] = useState("");
  const [validationMsg, setValidationMsg] = useState<{ valid: boolean; msg: string } | null>(null);

  const currentPubkey = puzzle?.publicKeyCompressed || manualPubkey || null;

  const handleValidate = useCallback(() => {
    if (!manualPubkey) {
      setValidationMsg({ valid: false, msg: "Enter a public key" });
      return;
    }
    const result = validatePublicKey(manualPubkey);
    if (result.valid) {
      setValidationMsg({ valid: true, msg: "Valid secp256k1 public key" });
      onPubkeyChange(manualPubkey, result.point || null);
    } else {
      setValidationMsg({ valid: false, msg: result.error || "Invalid public key" });
      onPubkeyChange(null, null);
    }
  }, [manualPubkey, onPubkeyChange]);

  const handleUsePuzzlePubkey = useCallback(() => {
    if (puzzle?.publicKeyCompressed) {
      const result = validatePublicKey(puzzle.publicKeyCompressed);
      if (result.valid) {
        setManualPubkey(puzzle.publicKeyCompressed);
        setValidationMsg({ valid: true, msg: "Using puzzle public key" });
        onPubkeyChange(puzzle.publicKeyCompressed, result.point || null);
      }
    }
  }, [puzzle, onPubkeyChange]);

  // Compute address if we have a pubkey
  const address = useMemo(() => {
    if (currentPubkey) {
      try {
        return pubkeyToAddress(currentPubkey);
      } catch {
        return null;
      }
    }
    return null;
  }, [currentPubkey]);

  return (
    <Card className="bg-zinc-900/60 border-zinc-800">
      <CardHeader className="pb-2 pt-3 px-4">
        <CardTitle className="text-xs font-semibold text-zinc-400 uppercase tracking-wider flex items-center gap-2">
          <Crosshair className="h-3.5 w-3.5 text-orange-400" />
          Target Input
        </CardTitle>
      </CardHeader>
      <CardContent className="px-4 pb-4 space-y-3">
        {puzzle && (
          <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-3 space-y-2">
            <div className="flex items-center gap-2">
              <span className="text-[9px] text-zinc-500 uppercase w-16">Puzzle:</span>
              <Badge
                variant="outline"
                className={`text-[9px] ${
                  puzzle.solved
                    ? "border-emerald-500/50 text-emerald-400"
                    : puzzle.publicKeyCompressed
                    ? "border-orange-500/50 text-orange-400"
                    : "border-red-500/50 text-red-400"
                }`}
              >
                #{puzzle.number}
              </Badge>
              <span className="text-[9px] text-zinc-500 font-mono">
                Range: {formatRange(puzzle.rangeStart, puzzle.rangeEnd)}
              </span>
            </div>
            {puzzle.publicKeyCompressed && (
              <div className="flex items-start gap-2">
                <span className="text-[9px] text-zinc-500 uppercase w-16 shrink-0">Pubkey:</span>
                <span className="text-[9px] text-orange-400 font-mono break-all">
                  {puzzle.publicKeyCompressed}
                </span>
              </div>
            )}
            {puzzle.address && (
              <div className="flex items-start gap-2">
                <span className="text-[9px] text-zinc-500 uppercase w-16 shrink-0">Address:</span>
                <span className="text-[9px] text-cyan-400 font-mono break-all">
                  {puzzle.address}
                </span>
              </div>
            )}
            {puzzle.publicKeyCompressed && (
              <Button
                size="sm"
                variant="outline"
                onClick={handleUsePuzzlePubkey}
                className="h-6 text-[9px] border-orange-600/50 text-orange-400 hover:text-orange-300"
              >
                Use This Pubkey as Target
              </Button>
            )}
          </div>
        )}

        {/* Manual pubkey input */}
        <div>
          <label className="text-[9px] text-zinc-500 uppercase mb-1 block">Manual Public Key Input (compressed, 66 hex chars)</label>
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

        {address && (
          <div className="text-[9px] font-mono text-zinc-500">
            Computed address: <span className="text-cyan-400">{address}</span>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// --- Section C: Solver Configuration ---

type Algorithm = "kangaroo" | "brute_force";

function SolverSection({
  puzzle,
  targetPubkeyHex,
  targetPubkeyPoint,
  onResult,
}: {
  puzzle: BitcoinPuzzle | null;
  targetPubkeyHex: string | null;
  targetPubkeyPoint: { x: bigint; y: bigint } | null;
  onResult: (result: { found: boolean; privateKey?: bigint; pipeline?: ReturnType<typeof computeFullPipeline> }) => void;
}) {
  const [algorithm, setAlgorithm] = useState<Algorithm>("kangaroo");
  const [maxIterations, setMaxIterations] = useState(10000000);
  const [isRunning, setIsRunning] = useState(false);
  const [isStopped, setIsStopped] = useState(false);
  const [progress, setProgress] = useState<KangarooProgress | null>(null);
  const [elapsed, setElapsed] = useState(0);
  const [log, setLog] = useState<string[]>([]);
  const abortRef = useRef(false);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const effectiveRangeStart = puzzle?.rangeStart ?? 0n;
  const effectiveRangeEnd = puzzle?.rangeEnd ?? 0n;

  const canUseKangaroo = !!targetPubkeyPoint && !!targetPubkeyHex;
  const rangeSize = puzzle ? getRangeSize(puzzle) : 0n;
  const rangeBits = puzzle ? puzzle.number : 0;

  // Estimate time
  const estimatedTime = useMemo(() => {
    if (!puzzle) return "N/A";
    if (algorithm === "kangaroo" && canUseKangaroo) {
      return formatTime(estimateTimeSeconds(effectiveRangeStart, effectiveRangeEnd));
    }
    if (algorithm === "brute_force") {
      // Brute force: range_size / 500000 seconds (very rough)
      const secs = Number(rangeSize) / 500000;
      return formatTime(secs);
    }
    return "N/A";
  }, [puzzle, algorithm, canUseKangaroo, effectiveRangeStart, effectiveRangeEnd, rangeSize]);

  const handleStart = useCallback(() => {
    if (!puzzle) return;
    if (algorithm === "kangaroo" && !canUseKangaroo) return;

    setIsRunning(true);
    setIsStopped(false);
    abortRef.current = false;
    setLog([]);
    setProgress(null);
    setElapsed(0);

    // Start timer
    const startTime = Date.now();
    timerRef.current = setInterval(() => {
      setElapsed(Date.now() - startTime);
    }, 100);

    // Run in chunks using setTimeout to avoid blocking UI
    if (algorithm === "kangaroo" && targetPubkeyPoint) {
      setLog((prev) => [...prev, `Starting Pollard's Kangaroo on puzzle #${puzzle.number}`]);
      setLog((prev) => [...prev, `Range: [2^${puzzle.number - 1}, 2^${puzzle.number})`]);
      setLog((prev) => [...prev, `Estimated time: ${estimatedTime}`]);

      // Run the algorithm with setTimeout to allow UI updates
      setTimeout(() => {
        try {
          const result = pollardKangaroo(
            targetPubkeyPoint,
            effectiveRangeStart,
            effectiveRangeEnd,
            maxIterations,
            (prog) => {
              if (abortRef.current) return;
              setProgress(prog);
              if (prog.iteration % 5000 === 0) {
                setLog((prev) => [
                  ...prev,
                  `Iter ${prog.iteration}: tame_dist=2^${prog.tameDistance.toString(2).length - 1}, wild_dist=2^${prog.wildDistance.toString(2).length - 1}, DPs: ${prog.tameDPs}T/${prog.wildDPs}W`,
                ]);
              }
            },
            1000
          );

          if (timerRef.current) clearInterval(timerRef.current);
          setElapsed(Date.now() - startTime);
          setIsRunning(false);

          if (result.found && result.privateKey !== undefined) {
            const keyHex = result.privateKey.toString(16).padStart(64, "0");
            const pipeline = computeFullPipeline(keyHex);
            setLog((prev) => [...prev, `FOUND! Private key: ${result.privateKey}`]);
            setLog((prev) => [...prev, `Address: ${pipeline.address}`]);
            onResult({ found: true, privateKey: result.privateKey, pipeline });
          } else {
            setLog((prev) => [...prev, `Not found after ${result.iterations} iterations (${result.timeMs}ms)`]);
            onResult({ found: false });
          }
        } catch (e) {
          if (timerRef.current) clearInterval(timerRef.current);
          setLog((prev) => [...prev, `Error: ${(e as Error).message}`]);
          setIsRunning(false);
        }
      }, 50);
    } else if (algorithm === "brute_force" && targetPubkeyHex) {
      setLog((prev) => [...prev, `Starting brute force on puzzle #${puzzle.number}`]);

      setTimeout(() => {
        try {
          const result = bruteForceSearch(
            targetPubkeyHex,
            effectiveRangeStart,
            effectiveRangeEnd,
            (current, checked) => {
              if (abortRef.current) return;
              if (checked % 1000 === 0) {
                setLog((prev) => [...prev, `Checked ${checked} keys, current: ${current}`]);
              }
            }
          );

          if (timerRef.current) clearInterval(timerRef.current);
          setElapsed(Date.now() - startTime);
          setIsRunning(false);

          if (result.found && result.privateKey !== undefined) {
            const keyHex = result.privateKey.toString(16).padStart(64, "0");
            const pipeline = computeFullPipeline(keyHex);
            setLog((prev) => [...prev, `FOUND! Private key: ${result.privateKey}`]);
            onResult({ found: true, privateKey: result.privateKey, pipeline });
          } else {
            setLog((prev) => [...prev, `Not found after checking ${result.checked} keys`]);
            onResult({ found: false });
          }
        } catch (e) {
          if (timerRef.current) clearInterval(timerRef.current);
          setLog((prev) => [...prev, `Error: ${(e as Error).message}`]);
          setIsRunning(false);
        }
      }, 50);
    }
  }, [puzzle, algorithm, canUseKangaroo, targetPubkeyHex, targetPubkeyPoint, effectiveRangeStart, effectiveRangeEnd, maxIterations, estimatedTime, onResult]);

  const handleStop = useCallback(() => {
    abortRef.current = true;
    setIsStopped(true);
    if (timerRef.current) clearInterval(timerRef.current);
    setIsRunning(false);
    setLog((prev) => [...prev, "Stopped by user"]);
  }, []);

  // Cleanup
  useEffect(() => {
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, []);

  return (
    <Card className="bg-zinc-900/60 border-zinc-800">
      <CardHeader className="pb-2 pt-3 px-4">
        <CardTitle className="text-xs font-semibold text-zinc-400 uppercase tracking-wider flex items-center gap-2">
          <Cpu className="h-3.5 w-3.5 text-cyan-400" />
          Solver Configuration
          <Badge variant="outline" className="text-[8px] border-zinc-700 text-zinc-500 ml-auto">
            JS BigInt • ~500K ops/sec
          </Badge>
        </CardTitle>
      </CardHeader>
      <CardContent className="px-4 pb-4 space-y-3">
        {/* Algorithm selector */}
        <div className="flex items-center gap-2 flex-wrap">
          <Button
            size="sm"
            variant={algorithm === "kangaroo" ? "default" : "outline"}
            onClick={() => setAlgorithm("kangaroo")}
            disabled={!canUseKangaroo}
            className={`h-7 text-[10px] ${
              algorithm === "kangaroo"
                ? "bg-orange-600 hover:bg-orange-500 text-white"
                : "border-zinc-700 text-zinc-400"
            }`}
          >
            <Zap className="h-3 w-3 mr-1" />
            Pollard&apos;s Kangaroo
          </Button>
          <Button
            size="sm"
            variant={algorithm === "brute_force" ? "default" : "outline"}
            onClick={() => setAlgorithm("brute_force")}
            className={`h-7 text-[10px] ${
              algorithm === "brute_force"
                ? "bg-cyan-600 hover:bg-cyan-500 text-white"
                : "border-zinc-700 text-zinc-400"
            }`}
          >
            <Search className="h-3 w-3 mr-1" />
            Brute Force
          </Button>
          {!canUseKangaroo && (
            <Badge variant="outline" className="text-[8px] border-orange-500/50 text-orange-400">
              Kangaroo requires known public key
            </Badge>
          )}
        </div>

        {/* Algorithm info */}
        <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-2 text-[9px] text-zinc-500">
          {algorithm === "kangaroo" ? (
            <>
              <span className="text-orange-400 font-semibold">Pollard&apos;s Kangaroo:</span> O(√n) time, O(1) space.
              Requires known public key. Best for ranges where pubkey is available.
              {canUseKangaroo && puzzle && (
                <div className="mt-1">
                  Expected iterations: ~{estimateKangarooIterations(effectiveRangeStart, effectiveRangeEnd).toLocaleString()} •
                  Est. time: <span className="text-orange-400">{estimatedTime}</span>
                </div>
              )}
            </>
          ) : (
            <>
              <span className="text-cyan-400 font-semibold">Brute Force:</span> O(n) time.
              Try every key in the range. Only feasible for very small ranges (&lt;40 bits).
              {puzzle && (
                <div className="mt-1">
                  Range size: 2^{rangeBits - 1} keys •
                  Est. time: <span className="text-cyan-400">{estimatedTime}</span>
                </div>
              )}
            </>
          )}
        </div>

        {/* Max iterations */}
        <div className="flex items-center gap-3">
          <label className="text-[9px] text-zinc-500 uppercase shrink-0">Max Iterations:</label>
          <Input
            type="number"
            value={maxIterations}
            onChange={(e) => setMaxIterations(parseInt(e.target.value) || 1000000)}
            className="font-mono text-[10px] bg-zinc-950 border-zinc-700 text-zinc-300 h-7 w-32"
          />
          <span className="text-[9px] text-zinc-600">Safety limit</span>
        </div>

        {/* Start/Stop buttons */}
        <div className="flex items-center gap-2">
          {!isRunning ? (
            <Button
              size="sm"
              onClick={handleStart}
              disabled={!puzzle || (algorithm === "kangaroo" && !canUseKangaroo)}
              className="h-8 text-[10px] bg-emerald-600 hover:bg-emerald-500 text-white"
            >
              <Play className="h-3 w-3 mr-1" />
              Start Search
            </Button>
          ) : (
            <Button
              size="sm"
              onClick={handleStop}
              className="h-8 text-[10px] bg-red-600 hover:bg-red-500 text-white"
            >
              <Square className="h-3 w-3 mr-1" />
              Stop
            </Button>
          )}
          {isRunning && (
            <div className="flex items-center gap-2">
              <svg className="animate-spin w-3 h-3 text-orange-400" viewBox="0 0 24 24">
                <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
                <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
              </svg>
              <span className="text-[9px] text-orange-400 font-mono">Running...</span>
            </div>
          )}
          {isStopped && (
            <Badge variant="outline" className="text-[8px] border-red-500/50 text-red-400">
              Stopped
            </Badge>
          )}
        </div>

        {/* Progress */}
        {(isRunning || progress) && (
          <div className="space-y-2">
            <div className="flex items-center gap-2">
              <Timer className="h-3 w-3 text-zinc-500" />
              <span className="text-[9px] text-zinc-500 font-mono">
                Elapsed: {(elapsed / 1000).toFixed(1)}s
              </span>
            </div>
            {progress && (
              <div className="grid grid-cols-2 sm:grid-cols-4 gap-2">
                <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-2 text-center">
                  <div className="text-[8px] text-zinc-500 uppercase">Iteration</div>
                  <div className="text-xs font-mono font-bold text-cyan-400">
                    {progress.iteration.toLocaleString()}
                  </div>
                </div>
                <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-2 text-center">
                  <div className="text-[8px] text-zinc-500 uppercase">Tame DPs</div>
                  <div className="text-xs font-mono font-bold text-orange-400">
                    {progress.tameDPs}
                  </div>
                </div>
                <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-2 text-center">
                  <div className="text-[8px] text-zinc-500 uppercase">Wild DPs</div>
                  <div className="text-xs font-mono font-bold text-emerald-400">
                    {progress.wildDPs}
                  </div>
                </div>
                <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-2 text-center">
                  <div className="text-[8px] text-zinc-500 uppercase">Speed</div>
                  <div className="text-xs font-mono font-bold text-zinc-300">
                    {elapsed > 0 ? ((progress.iteration / elapsed) * 1000).toFixed(0) : "—"}/s
                  </div>
                </div>
              </div>
            )}
          </div>
        )}

        {/* Log */}
        {log.length > 0 && (
          <div>
            <div className="text-[9px] text-zinc-500 uppercase mb-1">Solver Log</div>
            <div className="max-h-48 overflow-y-auto bg-zinc-950/80 border border-zinc-800 rounded-lg p-2 text-[8px] font-mono space-y-0.5">
              {log.map((entry, i) => (
                <div
                  key={i}
                  className={
                    entry.startsWith("FOUND")
                      ? "text-emerald-400 font-bold"
                      : entry.startsWith("Error")
                      ? "text-red-400"
                      : "text-zinc-500"
                  }
                >
                  {entry}
                </div>
              ))}
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// --- Section D: Fractal Analysis ---

function FractalAnalysisSection({
  targetPubkeyHex,
}: {
  targetPubkeyHex: string | null;
}) {
  const [analysis, setAnalysis] = useState<FullDiscreteAnalysis | null>(null);
  const [randomAnalysis, setRandomAnalysis] = useState<FullDiscreteAnalysis | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [progress, setProgress] = useState(0);
  const [activeFractalRound, setActiveFractalRound] = useState(0);

  const handleAnalyze = useCallback(() => {
    if (!targetPubkeyHex) return;

    setIsLoading(true);
    setProgress(0);

    setTimeout(() => {
      try {
        const pubkeyBlock = pubkeyToSha256Block(targetPubkeyHex);
        const pubAnalysis = computeFullDiscreteAnalysis(pubkeyBlock, (pct) => {
          setProgress(Math.round(pct * 0.5));
        });
        setAnalysis(pubAnalysis);

        const randomInput = generateRandomInput33();
        const randomBlock = input33ToSha256Block(randomInput);
        const rndAnalysis = computeFullDiscreteAnalysis(randomBlock, (pct) => {
          setProgress(50 + Math.round(pct * 0.5));
        });
        setRandomAnalysis(rndAnalysis);

        setProgress(100);
      } catch (e) {
        console.error("Fractal analysis error:", e);
      }
      setIsLoading(false);
    }, 50);
  }, [targetPubkeyHex]);

  return (
    <Card className="bg-zinc-900/60 border-zinc-800">
      <CardHeader className="pb-2 pt-3 px-4">
        <CardTitle className="text-xs font-semibold text-zinc-400 uppercase tracking-wider flex items-center gap-2">
          <Activity className="h-3.5 w-3.5 text-emerald-400" />
          SHA-256 Fractal Analysis on Target Pubkey
          <span className="text-[8px] text-zinc-600 font-mono normal-case ml-2">
            — Does SHA-256 treat this pubkey differently from random data?
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
                Analyzing...
              </span>
            ) : (
              <>
                <Hash className="h-3 w-3 mr-1" />
                Analyze SHA-256(target_pubkey)
              </>
            )}
          </Button>
          {isLoading && (
            <div className="space-y-1 flex-1">
              <Progress value={progress} className="h-1.5" />
              <span className="text-[9px] text-zinc-500 font-mono">{progress}%</span>
            </div>
          )}
        </div>

        {analysis && (
          <div>
            <div className="text-[9px] text-zinc-500 uppercase font-semibold mb-2 flex items-center gap-1">
              <span className="w-2 h-2 rounded-full bg-orange-400 inline-block" />
              SHA-256 on Puzzle Pubkey vs Random Input
            </div>
            <div className="grid grid-cols-1 xl:grid-cols-2 gap-4">
              <div>
                <div className="text-[8px] text-zinc-600 uppercase mb-1">Target Pubkey (Structured)</div>
                <DiscreteFractalPanel
                  analysis={analysis}
                  currentRound={activeFractalRound}
                  onRoundChange={setActiveFractalRound}
                />
              </div>
              {randomAnalysis && (
                <div>
                  <div className="text-[8px] text-zinc-600 uppercase mb-1">Random 33-byte Input</div>
                  <DiscreteFractalPanel
                    analysis={randomAnalysis}
                    currentRound={activeFractalRound}
                    onRoundChange={setActiveFractalRound}
                  />
                </div>
              )}
            </div>
          </div>
        )}

        {!analysis && !isLoading && targetPubkeyHex && (
          <div className="bg-zinc-950/40 border border-zinc-800/50 rounded-lg p-4 text-center text-[10px] text-zinc-600">
            Click the button above to analyze SHA-256&apos;s fractal behavior on this public key.
            This compares the structured EC point input against random data.
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// --- Section E: Results ---

function ResultsSection({
  result,
  puzzle,
}: {
  result: { found: boolean; privateKey?: bigint; pipeline?: ReturnType<typeof computeFullPipeline> } | null;
  puzzle: BitcoinPuzzle | null;
}) {
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback((text: string) => {
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }, []);

  if (!result) return null;

  if (!result.found) {
    return (
      <Card className="bg-zinc-900/60 border-zinc-800">
        <CardContent className="p-4 text-center">
          <div className="text-zinc-500 text-sm">No key found. Try increasing iterations or use a different algorithm.</div>
        </CardContent>
      </Card>
    );
  }

  const pipeline = result.pipeline;

  return (
    <Card className="bg-zinc-900/60 border-emerald-500/20 border">
      <CardHeader className="pb-2 pt-3 px-4">
        <CardTitle className="text-xs font-semibold text-emerald-400 uppercase tracking-wider flex items-center gap-2">
          <Key className="h-3.5 w-3.5 text-emerald-400" />
          Key Found!
          {puzzle && (
            <Badge variant="outline" className="text-[8px] border-emerald-500/50 text-emerald-400 ml-auto">
              Puzzle #{puzzle.number} SOLVED
            </Badge>
          )}
        </CardTitle>
      </CardHeader>
      <CardContent className="px-4 pb-4 space-y-3">
        {/* Private key */}
        <div className="bg-emerald-950/30 border border-emerald-500/30 rounded-lg p-3">
          <div className="flex items-center justify-between mb-1">
            <span className="text-[9px] text-emerald-400 uppercase font-semibold">Private Key (hex)</span>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => handleCopy(result.privateKey!.toString(16).padStart(64, "0"))}
              className="h-5 text-[8px] text-emerald-400 hover:text-emerald-300"
            >
              {copied ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />}
              {copied ? "Copied!" : "Copy"}
            </Button>
          </div>
          <div className="text-[10px] font-mono text-emerald-300 break-all">
            {result.privateKey!.toString(16).padStart(64, "0")}
          </div>
          <div className="mt-1 text-[9px] font-mono text-emerald-400/70">
            Decimal: {result.privateKey!.toString()}
          </div>
        </div>

        {/* Full pipeline verification */}
        {pipeline && (
          <div className="space-y-1.5">
            <div className="text-[9px] text-zinc-500 uppercase font-semibold">Full Pipeline Verification</div>
            <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-2 space-y-1.5 text-[9px] font-mono">
              <div className="flex items-start gap-2">
                <span className="text-zinc-600 w-28 shrink-0">Private Key:</span>
                <span className="text-emerald-400/70 break-all">{pipeline.privateKeyHex}</span>
              </div>
              <div className="flex items-start gap-2">
                <span className="text-zinc-600 w-28 shrink-0">Pubkey (comp):</span>
                <span className="text-cyan-400/70 break-all">{pipeline.publicKeyCompressedHex}</span>
              </div>
              <div className="flex items-start gap-2">
                <span className="text-zinc-600 w-28 shrink-0">SHA-256(pubkey):</span>
                <span className="text-orange-400/70 break-all">{pipeline.sha256Hex}</span>
              </div>
              <div className="flex items-start gap-2">
                <span className="text-zinc-600 w-28 shrink-0">RIPEMD-160(SHA):</span>
                <span className="text-amber-400/70 break-all">{pipeline.hash160Hex}</span>
              </div>
              <div className="flex items-start gap-2">
                <span className="text-zinc-600 w-28 shrink-0">Address:</span>
                <span className="text-cyan-300 break-all font-bold">{pipeline.address}</span>
              </div>
            </div>
          </div>
        )}

        {/* Verification badges */}
        <div className="flex items-center gap-2 flex-wrap">
          {pipeline && puzzle?.publicKeyCompressed && (
            <Badge
              variant="outline"
              className={`text-[8px] ${
                pipeline.publicKeyCompressedHex.toLowerCase() === puzzle.publicKeyCompressed.toLowerCase()
                  ? "border-emerald-500/50 text-emerald-400"
                  : "border-red-500/50 text-red-400"
              }`}
            >
              {pipeline.publicKeyCompressedHex.toLowerCase() === puzzle.publicKeyCompressed.toLowerCase()
                ? "✓ Pubkey Verified"
                : "✗ Pubkey Mismatch"}
            </Badge>
          )}
          {pipeline && puzzle?.address && (
            <Badge
              variant="outline"
              className={`text-[8px] ${
                pipeline.address === puzzle.address
                  ? "border-emerald-500/50 text-emerald-400"
                  : "border-red-500/50 text-red-400"
              }`}
            >
              {pipeline.address === puzzle.address ? "✓ Address Verified" : "✗ Address Mismatch"}
            </Badge>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

// --- Main Component ---

export default function PuzzleSolverPanel() {
  const [selectedPuzzle, setSelectedPuzzle] = useState<BitcoinPuzzle | null>(null);
  const [targetPubkeyHex, setTargetPubkeyHex] = useState<string | null>(null);
  const [targetPubkeyPoint, setTargetPubkeyPoint] = useState<{ x: bigint; y: bigint } | null>(null);
  const [solverResult, setSolverResult] = useState<{
    found: boolean;
    privateKey?: bigint;
    pipeline?: ReturnType<typeof computeFullPipeline>;
  } | null>(null);

  const handleSelectPuzzle = useCallback((puzzle: BitcoinPuzzle) => {
    setSelectedPuzzle(puzzle);
    setSolverResult(null);

    // Auto-set pubkey if available
    if (puzzle.publicKeyCompressed) {
      const result = validatePublicKey(puzzle.publicKeyCompressed);
      if (result.valid && result.point) {
        setTargetPubkeyHex(puzzle.publicKeyCompressed);
        setTargetPubkeyPoint(result.point);
      }
    } else {
      setTargetPubkeyHex(null);
      setTargetPubkeyPoint(null);
    }
  }, []);

  const handlePubkeyChange = useCallback((pubkeyHex: string | null, point: { x: bigint; y: bigint } | null) => {
    setTargetPubkeyHex(pubkeyHex);
    setTargetPubkeyPoint(point);
  }, []);

  const handleResult = useCallback((result: { found: boolean; privateKey?: bigint; pipeline?: ReturnType<typeof computeFullPipeline> }) => {
    setSolverResult(result);
  }, []);

  return (
    <div className="space-y-4">
      {/* Research context banner */}
      <div className="bg-zinc-950/80 border border-zinc-800/50 rounded-lg p-3 flex items-start gap-2">
        <AlertTriangle className="h-4 w-4 text-orange-400 mt-0.5 shrink-0" />
        <div className="text-[10px] text-zinc-400 leading-relaxed">
          <span className="text-orange-400 font-semibold">Proof of Concept:</span>{" "}
          This tool demonstrates Pollard&apos;s Kangaroo algorithm for the Bitcoin puzzle challenge.
          JavaScript BigInt performance is limited (~500K ops/sec vs billions on GPU/C++).
          For puzzles above ~50 bits, real solvers use specialized hardware.
          All computation is client-side — no data leaves your browser.
        </div>
      </div>

      {/* Section A: Puzzle Selector */}
      <PuzzleSelector selectedPuzzle={selectedPuzzle} onSelectPuzzle={handleSelectPuzzle} />

      {/* Section B: Target Input */}
      <TargetInputSection puzzle={selectedPuzzle} onPubkeyChange={handlePubkeyChange} />

      {/* Section C: Solver */}
      <SolverSection
        puzzle={selectedPuzzle}
        targetPubkeyHex={targetPubkeyHex}
        targetPubkeyPoint={targetPubkeyPoint}
        onResult={handleResult}
      />

      {/* Section E: Results */}
      <ResultsSection result={solverResult} puzzle={selectedPuzzle} />

      {/* Section D: Fractal Analysis */}
      <FractalAnalysisSection targetPubkeyHex={targetPubkeyHex} />
    </div>
  );
}
