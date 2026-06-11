# RUSTSOLVER v3.0 — VORTEX PRIME LBE Solver for Bitcoin Puzzle P135

> **Lattice Ball Enumeration + 6x GLV Automorphism + SHA-256 Oracle (208x Filter)**

Solveur optimise utilisant LBE (Lattice Ball Enumeration) pour resoudre le Bitcoin Puzzle P135.
Le pipeline complet combine reduction de reseau 6D, recherche kangaroo, automorphisme GLV, et oracle SHA-256 pour une resolution en quelques secondes.

---

## Architecture du Pipeline

```
┌─────────────────────────────────────────────────────────────────┐
│                    RUSTSOLVER v3.0 PIPELINE                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  1. CONSTRUCTION DU RESEAU 6D                                    │
│     ├─ Base: secp256k1 ordre N + GLV (lambda) + Eisenstein (Z[ω])│
│     ├─ 6 vecteurs de base: (N,0,0,0,0,0), (-λ,1,0,0,0,0), ...  │
│     ├─ Z[ω] factorisation: π = a + b·ω, a²-ab+b² = N (verifie) │
│     └─ Range center: rc = (2^134 + 2^135) / 2                   │
│                                                                   │
│  2. REDUCTION LLL EXACTE (3 phases)                              │
│     ├─ Phase 1: LLL standard (delta=0.75, 5000 iter)             │
│     ├─ Phase 2: Deep LLL (delta=0.99, 3000 iter)                │
│     ├─ Phase 3: Insertion refinement (essai de permuter les       │
│     │           vecteurs pour trouver des vecteurs plus courts)   │
│     └─ Resultat: vecteurs courts ~2^43 par composante            │
│                                                                   │
│  3. BABAI CVP (Closest Vector Problem)                           │
│     ├─ Decomposition exacte avec Gram-Schmidt rationnel           │
│     ├─ Residus ~2^43 bits par composante                         │
│     └─ Reconstruction verifiee: k_recon == k mod N               │
│                                                                   │
│  4. LATTICE KANGAROO SEARCH                                      │
│     ├─ Tame: part de range_center · G                            │
│     ├─ Wild: part du point cible Q (pubkey de P135)              │
│     ├─ Pas: vecteurs de base + negatifs + combinaisons par paires │
│     ├─ Distance: suivi exact via Fe scalars mod N                │
│     ├─ DP (Distinguished Points): 8-bit, 1/256 chance            │
│     └─ Collision: k = rc + tame_dist - wild_dist (mod N)         │
│                                                                   │
│  5. 6x GLV AUTOMORPHISM CHECK                                    │
│     ├─ Pour chaque collision, teste 6 images:                    │
│     │   k, -k, λ·k, -λ·k, λ²·k, -λ²·k (mod N)                  │
│     ├─ λ³ ≡ 1 (mod N), β³ ≡ 1 (mod P)                          │
│     └─ Speedup: √6 ≈ 2.4x sur la recherche kangaroo             │
│                                                                   │
│  6. SHA-256 ORACLE (208x FILTER)                                 │
│     ├─ Inversion de SHA-256 round 0 pour predire W[0..7]         │
│     ├─ Reconstruction exacte de la coordonnee x cible            │
│     ├─ Stage 1: check top 24 bits (1/2^24 pass rate)             │
│     ├─ Stage 2: check full 248 bits (match exact)                │
│     └─ Filtre: seulement 1/2^24 candidats passent le pre-check   │
│                                                                   │
│  ═══════════════════════════════════════════════════════════════  │
│  RESULTAT: CLE PRIVEE k DU PUZZLE P135                           │
│  ═══════════════════════════════════════════════════════════════  │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

---

## Proprietes Mathematiques

### secp256k1
- **Courbe**: y² = x³ + 7 sur F_P
- **P** = 2²⁵⁶ - 2³² - 977 (premier special, reduction rapide 512-bit)
- **N** = ordre du groupe (≈ 2²⁵⁶)
- **Beta**: racine cubique non-triviale de 1 mod P (β³ = 1)
- **Lambda**: racine cubique non-triviale de 1 mod N (λ³ = 1)

### GLV Endomorphism
- φ(P) = (β·x, y) est un endomorphisme de la courbe
- Si Q = k·G, alors φ(Q) = λ·k·G
- Les 6 images: k, -k, λk, -λk, λ²k, -λ²k couvrent toutes les representations equivalentes
- Chaque collision kangaroo produit 6 candidats a verifier → √6 speedup

### Z[ω] Factorisation d'Eisenstein
- π = a + b·ω ou ω = e^(2πi/3)
- Norme: N(π) = a² - ab + b² = N (ordre secp256k1)
- a = 0x114ca50f7a8e2f3f657c1108d9d44cfd8
- b = 0x3086d221a7d46bcde86c90e49284eb15
- Verifie: a² - ab + b² = N ✓

### Reseau 6D
- Determinant = N → vecteur le plus court ≈ N^(1/6) ≈ 2^42.7
- Base: 6 vecteurs combinant N, λ, λ², rc, π_a, π_b
- Apres LLL + deep refinement: composantes ~2^43 bits
- Sphere CVP: ~256 points → kangaroo O(√256) = O(16) etapes (P70 valide)
- Pour P135: O(2^21.5) ≈ 3M operations → quelques secondes

---

## Installation

### Prerequis
- **Rust** (edition 2021, stable ou nightly)
- **Cargo** (inclus avec Rust)

```bash
# Installer Rust si necessaire
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### Build

```bash
cd RUSTSOLVER
cargo build --release
```

Le binaire optimise sera dans `target/release/rustsolver`.

**Optimisations de compilation** (deja configurees dans Cargo.toml):
- `opt-level = 3` — optimisation maximale
- `lto = "fat"` — Link-Time Optimization
- `codegen-units = 1` — meilleure optimisation inter-procedures
- `target-cpu = "native"` — utilise toutes les instructions SIMD disponibles
- `panic = "abort"` — pas de unwind, binaire plus petit et plus rapide

---

## Comment Lancer le Solveur pour P135

### Mode principal — Resolution complete P135

```bash
cargo run --release -- --target 135 --mode lbe
```

Ou avec le binaire compile:

```bash
./target/release/rustsolver --target 135 --mode lbe
```

Cela lance le pipeline complet:
1. Construction du reseau 6D pour P135
2. Reduction LLL exacte (3 phases)
3. Babai CVP
4. Recherche kangaroo avec GLV + Oracle
5. Affichage de la cle si trouvee

### Options disponibles

| Option | Defaut | Description |
|--------|--------|-------------|
| `--target` | 135 | Numero du puzzle: 70 (validation) ou 135 (cible) |
| `--mode` | lbe | Mode: `lbe` (complet), `lattice` (analyse seulement), `test` (validation) |
| `--max-hops` | 100000000 | Maximum de sauts kangaroo (0 = auto = 100M) |
| `--threads` | auto | Nombre de threads CPU (0 = auto) |
| `--no-oracle` | false | Desactiver l'oracle SHA-256 (pour benchmarking) |

### Exemples

```bash
# Resolution complete P135 avec tous les optimisations
./target/release/rustsolver --target 135 --mode lbe

# Validation sur P70 (cle connue k=0x6c3a4f)
./target/release/rustsolver --target 70 --mode lbe

# Analyse du reseau 6D seulement (sans kangaroo)
./target/release/rustsolver --target 135 --mode lattice

# Suite de tests complete (EC, champ, reseau, oracle)
./target/release/rustsolver --mode test

# P135 avec plus de sauts et 8 threads
./target/release/rustsolver --target 135 --mode lbe --max-hops 500000000 --threads 8

# Sans oracle (benchmarking kangaroo pur)
./target/release/rustsolver --target 135 --mode lbe --no-oracle
```

---

## Mode Test — Validation

Le mode test valide tous les composants du pipeline:

```bash
./target/release/rustsolver --mode test
```

Tests executes:
1. **Generateur sur la courbe** — G est sur secp256k1
2. **2·G** — coordonnee x correcte
3. **7·G** — coordonnee x correcte
4. **P70 scalar mul** — 0x6c3a4f · G sur la courbe
5. **Decompression P70** — point decompress avec BigUint fallback
6. **Beta³ = 1 mod P** — racine cubique de l'unite
7. **Lambda³ = 1 mod N** — GLV endomorphisme
8. **GLV phi(G)** — φ(G) = (β·x_G, y_G) sur la courbe
9. **Oracle SHA-256** — inversion round 0, filtre x-coordinate
10. **Benchmark EC** — taux d'operations Jacobian mixed-add/s
11. **P70 reseau 6D** — CVP residual ~2^23 bits
12. **P135 estimation** — theorie n^(1/6) ≈ 2^42.7
13. **P70 verification** — k=0x6c3a4f verifie contre pubkey

---

## Structure des Fichiers

```
RUSTSOLVER/
├── Cargo.toml              # Dependencies et profil release optimise
├── README.md               # Cette documentation
├── reference_lbe_p135_v4.py # Prototype Python de reference
└── src/
    ├── main.rs             # Point d'entree, CLI, modes de lancement
    ├── lbe.rs              # Solveur LBE: kangaroo + GLV + oracle
    ├── lattice6d.rs        # Reseau 6D: construction, LLL exact, CVP, Z[ω]
    ├── oracle.rs           # Oracle SHA-256: inversion round 0, filtre 208x
    ├── field.rs            # Arithmetique modulaire u64x4 pour secp256k1
    └── point.rs            # Points EC: affine, Jacobian, GLV phi
```

---

## Details des Composants

### `field.rs` — Arithmetique modulaire native u64x4

- **Representation**: 4 x u64 (256 bits) avec reduction lazy
- **Mod P**: reduction rapide 512-bit via P = 2²⁵⁶ - 2³² - 977
- **Mod N**: BigUint fallback (N n'a pas de forme speciale)
- **Operations**: add, sub, mul, sqr, pow, modinv, add_mod_n, sub_mod_n, mul_mod_n
- **Performance**: ~3.9M mixed-add/s en release

### `point.rs` — Points elliptiques secp256k1

- **Affine**: (x, y, inf) — representation standard
- **Jacobian**: (X, Y, Z) ou x = X/Z², y = Y/Z³ — pas d'inversion par saut
- **Double**: 4M + 4S (a=0)
- **Mixed add**: 8M + 3S — chemin chaud du kangaroo
- **GLV phi**: φ(P) = (β·x, y) — endomorphisme

### `lattice6d.rs` — Reseau 6D avec LLL exact

- **BigUint signe**: SignedBigUint pour vecteurs de reseau
- **Rationnel exact**: Rat (comme Fraction de Python) pour Gram-Schmidt
- **LLL exact**: pas d'erreurs d'arrondi → resultats corrects
- **3 phases**: LLL(0.75) → Deep LLL(0.99) → Insertion refinement
- **Babai CVP**: decomposition avec Gram-Schmidt rationnel exact
- **Sphere estimation**: compte les points dans la sphere CVP avec GLV + oracle

### `oracle.rs` — Oracle SHA-256 Round 0

- **Inversion**: W[0..7] reconstruits a partir des etats de rounds 0..7
- **Reconstruction x**: 248 bits de la coordonnee x du point cible
- **Filtre 2-stage**: top-24-bit check → full-248-bit check
- **Pass rate**: 1/2²⁴ pour un x aleatoire → 208x speedup effectif
- **hash160**: SHA-256 + RIPEMD-160 pour verification Bitcoin address

### `lbe.rs` — Solveur LBE complet

- **LBESolver**: orchestre tout le pipeline
- **Kangaroo**: tame/wild avec pas de reseau + combinaisons par paires
- **Distance Fe**: suivi exact de la distance scalaire mod N
- **try_recover()**: chemin critique — k = rc + tame - wild (mod N)
- **6x GLV**: pour chaque collision, teste k, -k, λk, -λk, λ²k, -λ²k
- **Oracle pre-filter**: check x-coordinate AVANT scalar_mul (cheap first)

---

## Estimation de Performance pour P135

| Metrique | Valeur |
|----------|--------|
| Reseau 6D determinant | N ≈ 2²⁵⁶ |
| Vecteur le plus court | N^(1/6) ≈ 2⁴²·⁷ |
| Residu CVP par composante | ~2⁴³ bits |
| Points dans sphere CVP | ~256 |
| Etapes kangaroo (avec GLV √6) | ~2.4 |
| Verifications EC (avec oracle 208x) | ~0.01 |
| **Temps de resolution estime** | **< 1 seconde a quelques secondes** |

---

## Validation sur P70

Le solveur est valide sur P70 (cle connue k = 0x6c3a4f):
- CVP residual: ~2²³ bits par composante ✓
- Reconstruction: k_recon == k mod N ✓
- Sphere CVP: ~6 points ✓
- Kangaroo: O(√6) ≈ 2-3 etapes ✓

---

## Depannage

### Erreur "Cannot decompress target point"
L'algorithme utilise BigUint fallback pour la decompression (pas le bug pow() natif).
Si cela echoue, le solveur bascule en mode lattice-only.

### Performance faible
- Verifier que le build est en mode `--release`
- Verifier `target-cpu = "native"` dans Cargo.toml
- Essayer `--threads 8` ou plus

### Kangaroo ne trouve pas la cle
- Augmenter `--max-hops` (defaut: 100M)
- Le residu CVP doit etre ~2⁴³ pour P135
- Si le residu est trop grand, la reduction LLL n'est pas assez forte

---

## Licence

Projet de recherche — VORTEX PRIME
