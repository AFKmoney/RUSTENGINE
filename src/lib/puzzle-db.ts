/**
 * Bitcoin Puzzle Database
 *
 * Bitcoin puzzles are a well-known PUBLIC cryptographic challenge.
 * Someone sent BTC to addresses with private keys in known ranges [2^(n-1), 2^n).
 * The challenge is to find the private key for each range.
 *
 * Some puzzles have KNOWN public keys (making them more tractable via Pollard's Kangaroo),
 * others only have addresses (requiring brute force + address matching).
 */

export interface BitcoinPuzzle {
  number: number;          // Puzzle number (= bit range)
  rangeStart: bigint;      // 2^(n-1)
  rangeEnd: bigint;        // 2^n - 1
  publicKeyCompressed?: string;  // Known compressed pubkey hex (if available)
  address?: string;        // Bitcoin address (if known)
  balance?: number;        // Approximate BTC balance
  solved: boolean;
  solvedByKey?: string;    // Who solved it
}

// Helper to compute powers of 2
function pow2(n: number): bigint {
  return 1n << BigInt(n);
}

export const PUZZLES: BitcoinPuzzle[] = [
  // Solved puzzles
  { number: 1, rangeStart: pow2(0), rangeEnd: pow2(1) - 1n, publicKeyCompressed: "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798", address: "1BgGZ9tcN4rm9KBzDn7KprQz87SZ26SAMH", balance: 0.01, solved: true, solvedByKey: "Known (privkey=1)" },
  { number: 2, rangeStart: pow2(1), rangeEnd: pow2(2) - 1n, publicKeyCompressed: "02c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5", address: "1cMh228HTCiwS8ZsaakH8A8wze1JR5ZsP", balance: 0.02, solved: true, solvedByKey: "Known (privkey=3)" },
  { number: 3, rangeStart: pow2(2), rangeEnd: pow2(3) - 1n, publicKeyCompressed: "02f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9", address: "1QCBaArUsfMoMq5jQ9XpLrBBcB3LVKrYQ", balance: 0.03, solved: true, solvedByKey: "Known (privkey=7)" },
  { number: 4, rangeStart: pow2(3), rangeEnd: pow2(4) - 1n, solved: true },
  { number: 5, rangeStart: pow2(4), rangeEnd: pow2(5) - 1n, solved: true, solvedByKey: "Known (privkey=21)" },
  { number: 6, rangeStart: pow2(5), rangeEnd: pow2(6) - 1n, solved: true },
  { number: 7, rangeStart: pow2(6), rangeEnd: pow2(7) - 1n, solved: true },
  { number: 8, rangeStart: pow2(7), rangeEnd: pow2(8) - 1n, solved: true },
  { number: 9, rangeStart: pow2(8), rangeEnd: pow2(9) - 1n, solved: true },
  { number: 10, rangeStart: pow2(9), rangeEnd: pow2(10) - 1n, solved: true },
  { number: 11, rangeStart: pow2(10), rangeEnd: pow2(11) - 1n, solved: true },
  { number: 12, rangeStart: pow2(11), rangeEnd: pow2(12) - 1n, solved: true },
  { number: 13, rangeStart: pow2(12), rangeEnd: pow2(13) - 1n, solved: true },
  { number: 14, rangeStart: pow2(13), rangeEnd: pow2(14) - 1n, solved: true },
  { number: 15, rangeStart: pow2(14), rangeEnd: pow2(15) - 1n, solved: true },
  { number: 16, rangeStart: pow2(15), rangeEnd: pow2(16) - 1n, solved: true },
  { number: 17, rangeStart: pow2(16), rangeEnd: pow2(17) - 1n, solved: true },
  { number: 18, rangeStart: pow2(17), rangeEnd: pow2(18) - 1n, solved: true },
  { number: 19, rangeStart: pow2(18), rangeEnd: pow2(19) - 1n, solved: true },
  { number: 20, rangeStart: pow2(19), rangeEnd: pow2(20) - 1n, solved: true },
  { number: 21, rangeStart: pow2(20), rangeEnd: pow2(21) - 1n, solved: true },
  { number: 22, rangeStart: pow2(21), rangeEnd: pow2(22) - 1n, solved: true },
  { number: 23, rangeStart: pow2(22), rangeEnd: pow2(23) - 1n, solved: true },
  { number: 24, rangeStart: pow2(23), rangeEnd: pow2(24) - 1n, solved: true },
  { number: 25, rangeStart: pow2(24), rangeEnd: pow2(25) - 1n, solved: true },
  { number: 26, rangeStart: pow2(25), rangeEnd: pow2(26) - 1n, solved: true },
  { number: 27, rangeStart: pow2(26), rangeEnd: pow2(27) - 1n, solved: true },
  { number: 28, rangeStart: pow2(27), rangeEnd: pow2(28) - 1n, solved: true },
  { number: 29, rangeStart: pow2(28), rangeEnd: pow2(29) - 1n, solved: true },
  { number: 30, rangeStart: pow2(29), rangeEnd: pow2(30) - 1n, solved: true },
  { number: 31, rangeStart: pow2(30), rangeEnd: pow2(31) - 1n, solved: true },
  { number: 32, rangeStart: pow2(31), rangeEnd: pow2(32) - 1n, solved: true },
  { number: 33, rangeStart: pow2(32), rangeEnd: pow2(33) - 1n, solved: true },
  { number: 34, rangeStart: pow2(33), rangeEnd: pow2(34) - 1n, solved: true },
  { number: 35, rangeStart: pow2(34), rangeEnd: pow2(35) - 1n, solved: true },
  { number: 36, rangeStart: pow2(35), rangeEnd: pow2(36) - 1n, solved: true },
  { number: 37, rangeStart: pow2(36), rangeEnd: pow2(37) - 1n, solved: true },
  { number: 38, rangeStart: pow2(37), rangeEnd: pow2(38) - 1n, solved: true },
  { number: 39, rangeStart: pow2(38), rangeEnd: pow2(39) - 1n, solved: true },
  { number: 40, rangeStart: pow2(39), rangeEnd: pow2(40) - 1n, solved: true },
  { number: 41, rangeStart: pow2(40), rangeEnd: pow2(41) - 1n, solved: true },
  { number: 42, rangeStart: pow2(41), rangeEnd: pow2(42) - 1n, solved: true },
  { number: 43, rangeStart: pow2(42), rangeEnd: pow2(43) - 1n, solved: true },
  { number: 44, rangeStart: pow2(43), rangeEnd: pow2(44) - 1n, solved: true },
  { number: 45, rangeStart: pow2(44), rangeEnd: pow2(45) - 1n, solved: true },
  { number: 46, rangeStart: pow2(45), rangeEnd: pow2(46) - 1n, solved: true },
  { number: 47, rangeStart: pow2(46), rangeEnd: pow2(47) - 1n, solved: true },
  { number: 48, rangeStart: pow2(47), rangeEnd: pow2(48) - 1n, solved: true },
  { number: 49, rangeStart: pow2(48), rangeEnd: pow2(49) - 1n, solved: true },
  { number: 50, rangeStart: pow2(49), rangeEnd: pow2(50) - 1n, solved: true },
  { number: 51, rangeStart: pow2(50), rangeEnd: pow2(51) - 1n, solved: true },
  { number: 52, rangeStart: pow2(51), rangeEnd: pow2(52) - 1n, solved: true },
  { number: 53, rangeStart: pow2(52), rangeEnd: pow2(53) - 1n, solved: true },
  { number: 54, rangeStart: pow2(53), rangeEnd: pow2(54) - 1n, solved: true },
  { number: 55, rangeStart: pow2(54), rangeEnd: pow2(55) - 1n, solved: true },
  { number: 56, rangeStart: pow2(55), rangeEnd: pow2(56) - 1n, solved: true },
  { number: 57, rangeStart: pow2(56), rangeEnd: pow2(57) - 1n, solved: true },
  { number: 58, rangeStart: pow2(57), rangeEnd: pow2(58) - 1n, solved: true },
  { number: 59, rangeStart: pow2(58), rangeEnd: pow2(59) - 1n, solved: true },
  { number: 60, rangeStart: pow2(59), rangeEnd: pow2(60) - 1n, solved: true },
  { number: 61, rangeStart: pow2(60), rangeEnd: pow2(61) - 1n, solved: true },
  { number: 62, rangeStart: pow2(61), rangeEnd: pow2(62) - 1n, solved: true },
  { number: 63, rangeStart: pow2(62), rangeEnd: pow2(63) - 1n, solved: true },
  { number: 64, rangeStart: pow2(63), rangeEnd: pow2(64) - 1n, solved: true },
  { number: 65, rangeStart: pow2(64), rangeEnd: pow2(65) - 1n, solved: true },

  // Unsolved puzzles — #66 has known public key
  { number: 66, rangeStart: pow2(65), rangeEnd: pow2(66) - 1n, publicKeyCompressed: "0230210c23b1a047bc9bdbb13571e3b2df38de3c33c40551cdab43bd48e11b8cf2", address: "13zb1hQbWVsc2S7ZTZnP2G4undNNpdh5so", balance: 6.6, solved: false },
  { number: 67, rangeStart: pow2(66), rangeEnd: pow2(67) - 1n, publicKeyCompressed: "0294d991ef2a38291416f959de8f80769e0a74d7f81a49267f50b2de1a34dbc5df", address: "1BY8GQbnueYofwSuFAT3USAhGjPrkxDdW9", balance: 6.7, solved: false },
  { number: 68, rangeStart: pow2(67), rangeEnd: pow2(68) - 1n, address: "1MVDYgVaSN6iKKEsbzRUAYFhNJT1eLf2E3", balance: 6.8, solved: false },
  { number: 69, rangeStart: pow2(68), rangeEnd: pow2(69) - 1n, address: "1HsMJxNiV7TLxmoF6uJNkydxPFDog4NQC1", balance: 6.9, solved: false },
  { number: 70, rangeStart: pow2(69), rangeEnd: pow2(70) - 1n, address: "1KFHE7w8BhaENAswwryaoccDb6qcT6DbYY", balance: 7.0, solved: false },
  { number: 71, rangeStart: pow2(70), rangeEnd: pow2(71) - 1n, address: "1PWo3JeB9jrGwfHDNpdGK54CRas7fsVzXU", balance: 7.1, solved: false },
  { number: 72, rangeStart: pow2(71), rangeEnd: pow2(72) - 1n, address: "1JTK7s9YVYywfm5XUH7RNhHJH1LshCaRFR", balance: 7.2, solved: false },
  { number: 73, rangeStart: pow2(72), rangeEnd: pow2(73) - 1n, address: "12VVRNPi4SJqUTsp6FmqDqY5ehys8Yoqzp", balance: 7.3, solved: false },
  { number: 74, rangeStart: pow2(73), rangeEnd: pow2(74) - 1n, address: "1FWGcVDK3JGzCC3WtkYetULPszMaK2Jksv", balance: 7.4, solved: false },
  { number: 75, rangeStart: pow2(74), rangeEnd: pow2(75) - 1n, address: "1J36UjUByGroXcCvmj13U6uwaVv9caEeAt", balance: 7.5, solved: false },
  { number: 76, rangeStart: pow2(75), rangeEnd: pow2(76) - 1n, solved: false },
  { number: 77, rangeStart: pow2(76), rangeEnd: pow2(77) - 1n, solved: false },
  { number: 78, rangeStart: pow2(77), rangeEnd: pow2(78) - 1n, solved: false },
  { number: 79, rangeStart: pow2(78), rangeEnd: pow2(79) - 1n, solved: false },
  { number: 80, rangeStart: pow2(79), rangeEnd: pow2(80) - 1n, solved: false },
  { number: 81, rangeStart: pow2(80), rangeEnd: pow2(81) - 1n, solved: false },
  { number: 82, rangeStart: pow2(81), rangeEnd: pow2(82) - 1n, solved: false },
  { number: 83, rangeStart: pow2(82), rangeEnd: pow2(83) - 1n, solved: false },
  { number: 84, rangeStart: pow2(83), rangeEnd: pow2(84) - 1n, solved: false },
  { number: 85, rangeStart: pow2(84), rangeEnd: pow2(85) - 1n, solved: false },
  { number: 86, rangeStart: pow2(85), rangeEnd: pow2(86) - 1n, solved: false },
  { number: 87, rangeStart: pow2(86), rangeEnd: pow2(87) - 1n, solved: false },
  { number: 88, rangeStart: pow2(87), rangeEnd: pow2(88) - 1n, solved: false },
  { number: 89, rangeStart: pow2(88), rangeEnd: pow2(89) - 1n, solved: false },
  { number: 90, rangeStart: pow2(89), rangeEnd: pow2(90) - 1n, solved: false },
  { number: 91, rangeStart: pow2(90), rangeEnd: pow2(91) - 1n, solved: false },
  { number: 92, rangeStart: pow2(91), rangeEnd: pow2(92) - 1n, solved: false },
  { number: 93, rangeStart: pow2(92), rangeEnd: pow2(93) - 1n, solved: false },
  { number: 94, rangeStart: pow2(93), rangeEnd: pow2(94) - 1n, solved: false },
  { number: 95, rangeStart: pow2(94), rangeEnd: pow2(95) - 1n, solved: false },
  { number: 96, rangeStart: pow2(95), rangeEnd: pow2(96) - 1n, solved: false },
  { number: 97, rangeStart: pow2(96), rangeEnd: pow2(97) - 1n, solved: false },
  { number: 98, rangeStart: pow2(97), rangeEnd: pow2(98) - 1n, solved: false },
  { number: 99, rangeStart: pow2(98), rangeEnd: pow2(99) - 1n, solved: false },
  { number: 100, rangeStart: pow2(99), rangeEnd: pow2(100) - 1n, solved: false },
  { number: 101, rangeStart: pow2(100), rangeEnd: pow2(101) - 1n, solved: false },
  { number: 102, rangeStart: pow2(101), rangeEnd: pow2(102) - 1n, solved: false },
  { number: 103, rangeStart: pow2(102), rangeEnd: pow2(103) - 1n, solved: false },
  { number: 104, rangeStart: pow2(103), rangeEnd: pow2(104) - 1n, solved: false },
  { number: 105, rangeStart: pow2(104), rangeEnd: pow2(105) - 1n, solved: false },
  { number: 106, rangeStart: pow2(105), rangeEnd: pow2(106) - 1n, solved: false },
  { number: 107, rangeStart: pow2(106), rangeEnd: pow2(107) - 1n, solved: false },
  { number: 108, rangeStart: pow2(107), rangeEnd: pow2(108) - 1n, solved: false },
  { number: 109, rangeStart: pow2(108), rangeEnd: pow2(109) - 1n, solved: false },
  { number: 110, rangeStart: pow2(109), rangeEnd: pow2(110) - 1n, solved: false },
  { number: 111, rangeStart: pow2(110), rangeEnd: pow2(111) - 1n, solved: false },
  { number: 112, rangeStart: pow2(111), rangeEnd: pow2(112) - 1n, solved: false },
  { number: 113, rangeStart: pow2(112), rangeEnd: pow2(113) - 1n, solved: false },
  { number: 114, rangeStart: pow2(113), rangeEnd: pow2(114) - 1n, solved: false },
  { number: 115, rangeStart: pow2(114), rangeEnd: pow2(115) - 1n, solved: false },
  { number: 116, rangeStart: pow2(115), rangeEnd: pow2(116) - 1n, solved: false },
  { number: 117, rangeStart: pow2(116), rangeEnd: pow2(117) - 1n, solved: false },
  { number: 118, rangeStart: pow2(117), rangeEnd: pow2(118) - 1n, solved: false },
  { number: 119, rangeStart: pow2(118), rangeEnd: pow2(119) - 1n, solved: false },
  { number: 120, rangeStart: pow2(119), rangeEnd: pow2(120) - 1n, solved: false },
  { number: 121, rangeStart: pow2(120), rangeEnd: pow2(121) - 1n, solved: false },
  { number: 122, rangeStart: pow2(121), rangeEnd: pow2(122) - 1n, solved: false },
  { number: 123, rangeStart: pow2(122), rangeEnd: pow2(123) - 1n, solved: false },
  { number: 124, rangeStart: pow2(123), rangeEnd: pow2(124) - 1n, solved: false },
  { number: 125, rangeStart: pow2(124), rangeEnd: pow2(125) - 1n, solved: false },
  { number: 126, rangeStart: pow2(125), rangeEnd: pow2(126) - 1n, solved: false },
  { number: 127, rangeStart: pow2(126), rangeEnd: pow2(127) - 1n, solved: false },
  { number: 128, rangeStart: pow2(127), rangeEnd: pow2(128) - 1n, solved: false },
  { number: 129, rangeStart: pow2(128), rangeEnd: pow2(129) - 1n, solved: false },
  { number: 130, rangeStart: pow2(129), rangeEnd: pow2(130) - 1n, solved: false },
  { number: 131, rangeStart: pow2(130), rangeEnd: pow2(131) - 1n, solved: false },
  { number: 132, rangeStart: pow2(131), rangeEnd: pow2(132) - 1n, solved: false },
  { number: 133, rangeStart: pow2(132), rangeEnd: pow2(133) - 1n, solved: false },
  { number: 134, rangeStart: pow2(133), rangeEnd: pow2(134) - 1n, solved: false },
  { number: 135, rangeStart: pow2(134), rangeEnd: pow2(135) - 1n, solved: false },
  { number: 136, rangeStart: pow2(135), rangeEnd: pow2(136) - 1n, solved: false },
  { number: 137, rangeStart: pow2(136), rangeEnd: pow2(137) - 1n, solved: false },
  { number: 138, rangeStart: pow2(137), rangeEnd: pow2(138) - 1n, solved: false },
  { number: 139, rangeStart: pow2(138), rangeEnd: pow2(139) - 1n, solved: false },
  { number: 140, rangeStart: pow2(139), rangeEnd: pow2(140) - 1n, solved: false },
  { number: 141, rangeStart: pow2(140), rangeEnd: pow2(141) - 1n, solved: false },
  { number: 142, rangeStart: pow2(141), rangeEnd: pow2(142) - 1n, solved: false },
  { number: 143, rangeStart: pow2(142), rangeEnd: pow2(143) - 1n, solved: false },
  { number: 144, rangeStart: pow2(143), rangeEnd: pow2(144) - 1n, solved: false },
  { number: 145, rangeStart: pow2(144), rangeEnd: pow2(145) - 1n, solved: false },
  { number: 146, rangeStart: pow2(145), rangeEnd: pow2(146) - 1n, solved: false },
  { number: 147, rangeStart: pow2(146), rangeEnd: pow2(147) - 1n, solved: false },
  { number: 148, rangeStart: pow2(147), rangeEnd: pow2(148) - 1n, solved: false },
  { number: 149, rangeStart: pow2(148), rangeEnd: pow2(149) - 1n, solved: false },
  { number: 150, rangeStart: pow2(149), rangeEnd: pow2(150) - 1n, solved: false },
  { number: 151, rangeStart: pow2(150), rangeEnd: pow2(151) - 1n, solved: false },
  { number: 152, rangeStart: pow2(151), rangeEnd: pow2(152) - 1n, solved: false },
  { number: 153, rangeStart: pow2(152), rangeEnd: pow2(153) - 1n, solved: false },
  { number: 154, rangeStart: pow2(153), rangeEnd: pow2(154) - 1n, solved: false },
  { number: 155, rangeStart: pow2(154), rangeEnd: pow2(155) - 1n, solved: false },
  { number: 156, rangeStart: pow2(155), rangeEnd: pow2(156) - 1n, solved: false },
  { number: 157, rangeStart: pow2(156), rangeEnd: pow2(157) - 1n, solved: false },
  { number: 158, rangeStart: pow2(157), rangeEnd: pow2(158) - 1n, solved: false },
  { number: 159, rangeStart: pow2(158), rangeEnd: pow2(159) - 1n, solved: false },
  { number: 160, rangeStart: pow2(159), rangeEnd: pow2(160) - 1n, solved: false },
];

/**
 * Get a puzzle by its number
 */
export function getPuzzleByNumber(n: number): BitcoinPuzzle | undefined {
  return PUZZLES.find((p) => p.number === n);
}

/**
 * Get all unsolved puzzles
 */
export function getUnsolvedPuzzles(): BitcoinPuzzle[] {
  return PUZZLES.filter((p) => !p.solved);
}

/**
 * Get all puzzles with known public keys
 */
export function getPuzzlesWithPublicKey(): BitcoinPuzzle[] {
  return PUZZLES.filter((p) => p.publicKeyCompressed);
}

/**
 * Format a bigint range for display
 */
export function formatRange(start: bigint, end: bigint): string {
  const bits = end.toString(2).length;
  if (bits <= 10) {
    return `[${start}, ${end}]`;
  }
  return `[2^${bits - 1}, 2^${bits})`;
}

/**
 * Get the range size as a bigint
 */
export function getRangeSize(puzzle: BitcoinPuzzle): bigint {
  return puzzle.rangeEnd - puzzle.rangeStart + 1n;
}

/**
 * Estimate the number of Kangaroo operations needed
 * For Pollard's Kangaroo: ~2.5 * sqrt(range_size)
 */
export function estimateKangarooOps(puzzle: BitcoinPuzzle): bigint {
  const rangeSize = getRangeSize(puzzle);
  // sqrt of range_size
  const sqrtSize = sqrtBigInt(rangeSize);
  return 3n * sqrtSize; // conservative estimate
}

/**
 * Estimate time in seconds for Kangaroo algorithm
 * Assumes ~500K point additions/sec in JS BigInt (conservative)
 */
export function estimateKangarooTimeSec(puzzle: BitcoinPuzzle): number {
  const ops = estimateKangarooOps(puzzle);
  const opsPerSec = 500_000;
  return Number(ops) / opsPerSec;
}

/**
 * BigInt square root (integer)
 */
function sqrtBigInt(n: bigint): bigint {
  if (n < 0n) throw new Error("Cannot compute sqrt of negative number");
  if (n < 2n) return n;
  let x = n;
  let y = (x + 1n) / 2n;
  while (y < x) {
    x = y;
    y = (x + n / x) / 2n;
  }
  return x;
}
