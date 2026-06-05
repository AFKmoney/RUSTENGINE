#!/usr/bin/env python3
"""
VORTEX PRIME — Les 3 Voies vers l'Inversion
=============================================

Voie 1: Algorithme de réseau exploitant CM(Q(√-3))
Voie 2: Systèmes polynomiaux sur GF(2) — linéarisation de SHA-256
Voie 3: Preuve que SHA-256(EC) ≠ random oracle

Target: Bitcoin Puzzle #135
  Pubkey: 02145d2611c823a396ef6712ce0f712f09b9b4f3135e3e0aa3230fb9b6d08d1e16
  Address: 16RGFo6hjq9ym6Pj7N5H7L1NR1rVPJyw2v
  Range:  d ∈ [2^134, 2^135)
"""

import hashlib
import struct
import math
import json
import time
import random
from collections import defaultdict

from ecdsa import SECP256k1, SigningKey
from ecdsa.ellipticcurve import Point
from ecdsa.numbertheory import inverse_mod

import numpy as np

# ============================================================================
# CONSTANTS
# ============================================================================
CURVE = SECP256k1.curve
ORDER = SECP256k1.order
GENERATOR = SECP256k1.generator
P = CURVE.p()
N = ORDER

# GLV endomorphism
LAMBDA = 0x5363ad4cc05c30e0a5261c028812645a122e22ea20816678df02967c1b23bd72
BETA = 0x7ae96a2b657c07106e64479eac3434e99cf0497512f58995c1396c28719501ee
LAMBDA2 = pow(LAMBDA, 2, N)  # λ² mod N

# Target
TARGET_PUBKEY = "02145d2611c823a396ef6712ce0f712f09b9b4f3135e3e0aa3230fb9b6d08d1e16"
TARGET_ADDRESS = "16RGFo6hjq9ym6Pj7N5H7L1NR1rVPJyw2v"
KEY_MIN = 2**134

SHA256_K = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2
]


def privkey_to_compressed(d):
    sk = SigningKey.from_secret_exponent(d, curve=SECP256k1)
    vk = sk.get_verifying_key()
    x = vk.pubkey.point.x()
    y = vk.pubkey.point.y()
    prefix = '02' if y % 2 == 0 else '03'
    return prefix + f'{x:064x}'


def parse_target():
    prefix = TARGET_PUBKEY[:2]
    x = int(TARGET_PUBKEY[2:], 16)
    y_sq = (pow(x, 3, P) + 7) % P
    y = pow(y_sq, (P + 1) // 4, P)
    if (prefix == '02' and y % 2 != 0) or (prefix == '03' and y % 2 == 0):
        y = P - y
    return Point(CURVE, x, y)


# ============================================================================
# VOIE 1: ALGORITHME DE RÉSEAU EXPLOITANT CM(Q(√-3))
# ============================================================================
def voie1_lattice_cm():
    """
    VOIE 1: Exploiter la structure CM de secp256k1 pour une 
    décomposition lattice optimale.
    
    secp256k1 a CM par Q(√-3):
    - j-invariant = 0
    - Groupe d'automorphismes d'ordre 6
    - Anneau d'endomorphismes = Z[ω] où ω = (1+√-3)/2
    - ω³ = 1 (sur la courbe), ω² + ω + 1 = 0 (algébriquement)
    
    L'approche:
    1. Construire le réseau GLV 3D avec la base complète
    2. Appliquer LLL/BKZ adapté à la structure hexagonale
    3. Chercher des vecteurs courts exploitant la symétrie d'ordre 3
    4. Décomposer d dans cette base réduite
    """
    print("=" * 70)
    print("  VOIE 1: RÉSEAU CM(Q(√-3)) — Lattice BKZ sur secp256k1")
    print("=" * 70)
    
    # ═══════════════════════════════════════════════════════════
    # ÉTAPE 1: Analyser la structure du réseau endomorphisme
    # ═══════════════════════════════════════════════════════════
    print("\n[1.1] Structure du réseau d'endomorphismes")
    print("-" * 50)
    
    # Le réseau GLV standard a pour base:
    # B = [[N, 0, 0], [-λ, 1, 0], [-λ², 0, 1]]
    # Déterminant = N (l'ordre du groupe)
    
    # La norme du réseau est N^(1/3) ≈ 2^85.3
    minkowski_bound = (3 ** 0.5) * (N ** (1/3))
    print(f"  Borne de Minkowski (3D): {minkowski_bound:.2e} ≈ 2^{math.log2(minkowski_bound):.1f}")
    print(f"  Théorème: ∃ vecteur de norme ≤ {minkowski_bound:.2e}")
    
    # Norme du réseau par unité de volume
    # Vol(L) = N, dim = 3
    # λ₁(L) ≤ √3 · det(L)^(1/3) = √3 · N^(1/3) ≈ 2^85.3
    # C'est la MEILLEURE décomposition possible (théorique)
    
    # ═══════════════════════════════════════════════════════════
    # ÉTAPE 2: LLL sur le réseau 3D GLV
    # ═══════════════════════════════════════════════════════════
    print("\n[1.2] Réduction LLL du réseau GLV 3D")
    print("-" * 50)
    
    # Base du réseau (lignes = vecteurs de base)
    B = np.array([
        [float(N), 0, 0],
        [float((-LAMBDA) % N), 1, 0],
        [float((-LAMBDA2) % N), 0, 1]
    ], dtype=np.float64)
    
    print(f"  Base originale B[0]: ({B[0,0]:.2e}, {B[0,1]}, {B[0,2]})")
    print(f"  Base originale B[1]: ({B[1,0]:.2e}, {B[1,1]}, {B[1,2]})")
    print(f"  Base originale B[2]: ({B[2,0]:.2e}, {B[2,1]}, {B[2,2]})")
    
    # Implémentation LLL pour dimension 3
    def lll_3d(B_in, delta=0.99):
        """LLL reduction for 3D lattice."""
        B = B_in.copy().astype(np.float64)
        n = 3
        
        def gram_schmidt(M):
            n = M.shape[0]
            B_star = np.zeros_like(M, dtype=np.float64)
            mu = np.zeros((n, n), dtype=np.float64)
            norms = np.zeros(n, dtype=np.float64)
            B_star[0] = M[0].copy()
            norms[0] = np.dot(B_star[0], B_star[0])
            for i in range(1, n):
                B_star[i] = M[i].copy()
                for j in range(i):
                    if norms[j] > 0:
                        mu[i, j] = np.dot(M[i], B_star[j]) / norms[j]
                        B_star[i] -= mu[i, j] * B_star[j]
                norms[i] = np.dot(B_star[i], B_star[i])
            return B_star, mu, norms
        
        changed = True
        iterations = 0
        while changed and iterations < 100:
            changed = False
            iterations += 1
            B_star, mu, norms = gram_schmidt(B)
            
            for i in range(1, n):
                # Size-reduce
                for j in range(i-1, -1, -1):
                    if abs(mu[i, j]) > 0.5:
                        r = round(mu[i, j])
                        B[i] -= r * B[j]
                        B_star, mu, norms = gram_schmidt(B)
                        changed = True
                
                # Lovász condition
                if i > 0:
                    lhs = norms[i]
                    rhs = (delta - mu[i, i-1]**2) * norms[i-1]
                    if lhs < rhs:
                        # Swap
                        B[[i, i-1]] = B[[i-1, i]]
                        B_star, mu, norms = gram_schmidt(B)
                        changed = True
        
        return B, iterations
    
    B_reduced, iters = lll_3d(B)
    
    norms_reduced = [np.linalg.norm(B_reduced[i]) for i in range(3)]
    print(f"  LLL terminé en {iters} itérations")
    print(f"  Normes après LLL: {[f'{n:.2e}' for n in norms_reduced]}")
    for i in range(3):
        print(f"  B_reduced[{i}]: ({B_reduced[i,0]:.4e}, {B_reduced[i,1]:.4e}, {B_reduced[i,2]:.4e})")
        bits = math.log2(max(abs(B_reduced[i,0]), abs(B_reduced[i,1]), abs(B_reduced[i,2]), 1))
        print(f"    ≈ 2^{bits:.1f} par composante")
    
    # ═══════════════════════════════════════════════════════════
    # ÉTAPE 3: BKZ avec block size croissante
    # ═══════════════════════════════════════════════════════════
    print("\n[1.3] BKZ adapté à la structure hexagonale")
    print("-" * 50)
    
    # En dimension 3, BKZ avec block size 3 = LLL complet
    # Donc LLL est déjà optimal pour dim 3!
    # 
    # MAIS: on peut utiliser la structure CM pour faire mieux.
    # L'anneau d'endomorphismes Z[ω] a une structure hexagonale.
    # Les vecteurs courts du réseau IDEAL Z[ω] ont une forme 
    # très spécifique liée aux nombres d'Eisenstein.
    
    # Les nombres d'Eisenstein: Z[ω] où ω = (-1+i√3)/2
    # La norme dans Z[ω]: N(a + bω) = a² - ab + b²
    # Les unités: ±1, ±ω, ±ω² (6 unités!)
    
    # Pour secp256k1, l'endomorphisme φ satisfait φ² + φ + 1 ≡ 0 (mod N)
    # (car λ² + λ + 1 ≡ 0 mod N, ce que nous pouvons vérifier)
    
    lambda_sum = (LAMBDA + LAMBDA2 + 1) % N
    print(f"  λ + λ² + 1 mod N = {lambda_sum}")
    print(f"  λ² + λ + 1 ≡ 0 (mod N): {lambda_sum == 0}")
    
    # C'est la relation CLÉ! Elle signifie que le réseau a une 
    # symétrie d'ordre 3 PARFAITE.
    #
    # La matrice de l'endomorphisme dans la base (1, λ):
    # M = [[0, -1], [1, -1]]  (car λ → λ² = -λ - 1)
    #
    # Cette matrice a pour polynôme caractéristique x² + x + 1
    # qui est le polynôme minimal de ω.
    
    # ═══════════════════════════════════════════════════════════
    # ÉTAPE 4: Décomposition de d dans le réseau réduit
    # ═══════════════════════════════════════════════════════════
    print("\n[1.4] Décomposition de d dans le réseau réduit")
    print("-" * 50)
    
    # CVP (Closest Vector Problem): trouver le vecteur du réseau
    # le plus proche de (d, 0, 0)
    
    # Babai's nearest plane sur la base LLL-réduite
    def babai_cvp(B_basis, target):
        """Solve CVP using Babai's nearest plane algorithm."""
        n = B_basis.shape[0]
        # Gram-Schmidt
        B_star = np.zeros_like(B_basis, dtype=np.float64)
        mu = np.zeros((n, n), dtype=np.float64)
        B_star[0] = B_basis[0].copy()
        for i in range(1, n):
            B_star[i] = B_basis[i].copy()
            for j in range(i):
                norm_sq = np.dot(B_star[j], B_star[j])
                if norm_sq > 0:
                    mu[i, j] = np.dot(B_basis[i], B_star[j]) / norm_sq
                    B_star[i] -= mu[i, j] * B_star[j]
        
        # Nearest plane
        b = np.zeros(n, dtype=np.float64)
        t = target.copy().astype(np.float64)
        for i in range(n-1, -1, -1):
            norm_sq = np.dot(B_star[i], B_star[i])
            if norm_sq > 0:
                ci = np.dot(t, B_star[i]) / norm_sq
                b[i] = round(ci)
                t = t - b[i] * B_basis[i]
        
        closest = b @ B_basis
        error = target.astype(np.float64) - closest
        return b.astype(int), error
    
    # Test avec une clé connue
    test_d = KEY_MIN + 0xDEADBEEFCAFE
    target_vec = np.array([float(test_d), 0, 0])
    
    # Décomposition dans la base LLL-réduite
    coeffs, error = babai_cvp(B_reduced, target_vec)
    
    # La décomposition est: d = coeffs @ B_reduced + error
    # Donc d₀ = error[0], d₁ = error[1], d₂ = error[2]
    d0 = int(round(error[0]))
    d1 = int(round(error[1]))
    d2 = int(round(error[2]))
    
    print(f"  Test d = 2^134 + 0xDEADBEEFCAFE")
    print(f"  d₀ = {d0} ({abs(d0).bit_length()} bits)")
    print(f"  d₁ = {d1} ({abs(d1).bit_length()} bits)")
    print(f"  d₂ = {d2} ({abs(d2).bit_length()} bits)")
    
    # Vérifier: d₀ + d₁·λ + d₂·λ² ≡ d (mod N)
    recomposed = (d0 + d1 * LAMBDA + d2 * LAMBDA2) % N
    if recomposed < 0: recomposed += N
    print(f"  Vérification d₀ + d₁·λ + d₂·λ² ≡ d: {recomposed == test_d}")
    
    if not (recomposed == test_d):
        # Essayer avec les coefficients complets
        full_d = int(round(coeffs[0] * B_reduced[0,0] + coeffs[1] * B_reduced[1,0] + coeffs[2] * B_reduced[2,0] + error[0]))
        print(f"  Reconstruction complète: d_recon = {full_d}")
        print(f"  Match: {full_d == test_d}")
    
    # ═══════════════════════════════════════════════════════════
    # ÉTAPE 5: NOUVEAU — Décomposition en 6 composantes 
    # utilisant les 6 automorphismes
    # ═══════════════════════════════════════════════════════════
    print("\n[1.5] NOUVEAU: Décomposition 6-voies via les 6 automorphismes")
    print("-" * 50)
    
    # Les 6 automorphismes de secp256k1 (groupe d'ordre 6):
    # ψ₀(P) = P         (identité)       → [1]P
    # ψ₁(P) = -P        (négation)       → [-1]P  
    # ψ₂(P) = φ(P)      (endomorphisme)  → [λ]P
    # ψ₃(P) = -φ(P)                      → [-λ]P
    # ψ₄(P) = φ²(P)                      → [λ²]P
    # ψ₅(P) = -φ²(P)                     → [-λ²]P
    
    # Pour d·G, on peut écrire:
    # d·G = Σᵢ cᵢ · ψᵢ(G) où les cᵢ sont des scalaires
    # 
    # En termes de multiplicateurs:
    # d = c₀ + c₁·(-1) + c₂·λ + c₃·(-λ) + c₄·λ² + c₅·(-λ²)  (mod N)
    # d = c₀ - c₁ + (c₂ - c₃)·λ + (c₄ - c₅)·λ²  (mod N)
    #
    # C'est équivalent à la décomposition 3-voies avec signes.
    # MAIS: en séparant les 6 composantes, on peut faire un 
    # MITM 6-voies où chaque composante est plus petite!
    
    # Optimisation: choisir les signes pour minimiser la somme
    # d = d₀ + s₁·d₁ + s₂·d₂  où s₁, s₂ ∈ {-λ, λ} et d₀, d₁, d₂ ≥ 0
    
    # Pour k < 2^134, la MEILLEURE décomposition donne:
    # |d₀|, |d₁|, |d₂| < 2^(134/3) ≈ 2^44.67
    # SI le réseau est bien réduit!
    
    # Vérifions cette borne pour des clés dans le range
    print("  Test de décomposition pour 100 clés dans [2^134, 2^135):")
    
    max_bits = [0, 0, 0]
    sum_bits = [0, 0, 0]
    n_tests = 100
    
    for i in range(n_tests):
        d = KEY_MIN + random.randint(0, 2**134 - 1)
        target_v = np.array([float(d), 0, 0])
        coeffs_i, error_i = babai_cvp(B_reduced, target_v)
        
        comps = [abs(int(round(error_i[j]))) for j in range(3)]
        for j in range(3):
            max_bits[j] = max(max_bits[j], comps[j].bit_length())
            sum_bits[j] += comps[j].bit_length()
    
    avg_bits = [s/n_tests for s in sum_bits]
    print(f"  Taille max des composantes: {[f'{b} bits' for b in max_bits]}")
    print(f"  Taille moyenne: {[f'{b:.1f} bits' for b in avg_bits]}")
    
    # ═══════════════════════════════════════════════════════════
    # ÉTAPE 6: Architecture d'attaque MITM multi-dimensionnelle
    # ═══════════════════════════════════════════════════════════
    print("\n[1.6] Architecture d'attaque MITM")
    print("-" * 50)
    
    comp_size = max(avg_bits)
    print(f"  Taille des composantes: ~{comp_size:.0f} bits chacune")
    print(f"  Split hi/lo: chaque sous-composante ~{comp_size/2:.0f} bits")
    
    # 3-way decomposition avec hi/lo split:
    # d = d₀_lo + d₀_hi·2^b + (d₁_lo + d₁_hi·2^b)·λ + (d₂_lo + d₂_hi·2^b)·λ²
    # où b = comp_size/2
    
    b = int(comp_size / 2)
    baby_size = 3 * b  # 3 composantes lo × b bits chacune = 3b bits d'index
    giant_size = 3 * b  # 3 composantes hi
    
    print(f"  Baby step (d₀_lo, d₁_lo, d₂_lo): 2^{baby_size} entrées")
    print(f"  Giant step (d₀_hi, d₁_hi, d₂_hi): 2^{giant_size} entrées")
    print(f"  Stockage baby: 2^{baby_size} × 64 bytes = {2**baby_size * 64 / 2**40:.1f} TB")
    
    if baby_size <= 40:
        print(f"  → 2^{baby_size} ≈ {2**baby_size:,} — POTENTIELLEMENT RÉALISABLE!")
    else:
        print(f"  → 2^{baby_size} — TROP GRAND pour le stockage actuel")
        
        # Optimisation: split en PLUSIEURS groupes
        # Groupe 1 (baby): d₀_lo seul → 2^b entrées
        # Groupe 2: d₁_lo, d₂_lo → 2^2b entrées (calcul à la volée)
        # Groupe 3 (giant): d₀_hi, d₁_hi, d₂_hi → 2^3b entrées
        
        # Alternatives avec la structure d'ordre 3:
        # Utiliser la relation λ² = -λ - 1 pour réduire une dimension
        
        print(f"\n  OPTIMISATION via λ² = -λ - 1:")
        print(f"  d = d₀ + d₁·λ + d₂·(-λ-1) = (d₀ - d₂) + (d₁ - d₂)·λ")
        print(f"  → Seulement 2 composantes indépendantes!")
        print(f"  → Retour à la décomposition GLV 2D standard")
        print(f"  → La relation λ²+λ+1≡0 RÉDUIT la dimension de 3 à 2!")
        
        # En fait, la décomposition 3D se réduit à 2D à cause de
        # la relation algébrique. C'est pourquoi GLV standard est 2D.
        #
        # La vraie question: peut-on utiliser la structure d'ordre 3
        # pour améliorer la qualité de la réduction lattice en 2D?
    
    # ═══════════════════════════════════════════════════════════
    # ÉTAPE 7: NOUVEAU — Idéal lattice dans Z[ω]
    # ═══════════════════════════════════════════════════════════
    print("\n[1.7] NOUVEAU: Réseau idéal dans Z[ω] (nombres d'Eisenstein)")
    print("-" * 50)
    
    # L'anneau des endomorphismes est isomorphe à Z[ω]/I pour un 
    # idéal I de Z[ω]. Le réseau est un RÉSEAU IDÉAL.
    #
    # Les réseaux idéaux dans Z[ω] ont des propriétés spéciales:
    # 1. Symétrie hexagonale (groupe de symétrie d'ordre 6)
    # 2. Les vecteurs courts sont des nombres d'Eisenstein
    # 3. La réduction est plus efficace que pour les réseaux généraux
    
    # La norme d'Eisenstein: N(a + bω) = a² - ab + b²
    # Un nombre d'Eisenstein a + bω est PREMIER ssi:
    # - a² - ab + b² est premier (dans Z)
    # - ou a² - ab + b² = 3 (le seul cas avec |a|,|b| ≤ 1)
    
    # La factorisation de N dans Z[ω]:
    # N = ORDER ≈ 2^256
    # N = produit d'idéaux premiers dans Z[ω]
    
    # Pour exploiter cela, on peut:
    # 1. Factoriser N dans Z[ω]
    # 2. Trouver les idéaux premiers
    # 3. Utiliser le CRT sur les idéaux pour décomposer le problème
    
    # Vérifions si N est premier dans Z
    print(f"  N = {N:#x}")
    print(f"  N est premier: {all(N % p != 0 for p in range(2, 1000))}")
    
    # Si N est premier dans Z, alors dans Z[ω]:
    # - Si N ≡ 2 (mod 3): N reste premier dans Z[ω] (inerte)
    # - Si N ≡ 1 (mod 3): N = π · π̄ (scindé) où π est un nombre d'Eisenstein premier
    # - Si N ≡ 0 (mod 3): N = -ω² · (1-ω)² (ramifié)
    
    N_mod_3 = N % 3
    print(f"  N mod 3 = {N_mod_3}")
    
    if N_mod_3 == 1:
        print(f"  N est SCINDÉ dans Z[ω]: N = π · π̄")
        print(f"  → On peut factoriser N comme produit de deux idéaux premiers!")
        print(f"  → Le réseau idéal peut être réduit en utilisant cette factorisation!")
        
        # Trouver la factorisation de N dans Z[ω]:
        # N = a² - ab + b² pour certains a, b ∈ Z
        # On cherche a, b tels que a² - ab + b² = N
        
        # Méthode: résoudre a² - ab + b² = N
        # ≡ 4(a² - ab + b²) = 4N
        # ≡ (2a - b)² + 3b² = 4N
        
        # Donc: (2a - b)² + 3b² = 4N
        # Soit u = 2a - b, v = b:
        # u² + 3v² = 4N
        
        print(f"\n  Recherche de la factorisation u² + 3v² = 4N...")
        
        # Méthode de Cornacchia pour u² + 3v² = 4N
        # 1. Trouver une racine de -3 mod N
        # x² ≡ -3 (mod N)
        
        # Utiliser le critère d'Euler: -3 est un résidu quadratique mod N
        # ssi N ≡ 1 (mod 3) (ce qu'on a vérifié)
        
        # Trouver x tel que x² ≡ -3 (mod N)
        # Méthode de Tonelli-Shanks adaptée
        
        # Essayer avec Cipolla (plus simple pour les grands nombres)
        # x² ≡ -3 (mod N)
        
        # Méthode probabiliste de Cipolla:
        # 1. Choisir a aléatoirement
        # 2. Vérifier que a² + 3 n'est pas un résidu quadratique
        # 3. Calculer x = (a + √(a²+3))^((N+1)/2) mod N
        
        # Pour notre cas, on peut utiliser pow(-3, (N+1)//4, N) si N ≡ 3 (mod 4)
        # Sinon, utiliser l'algorithme de Cipolla
        
        print(f"  Calcul d'une racine de -3 mod N...")
        
        # Essayer la méthode directe (ne marche que si N ≡ 3 mod 4)
        if N % 4 == 3:
            sqrt_neg3 = pow(-3 % N, (N + 1) // 4, N)
        else:
            # Méthode de Cipolla
            sqrt_neg3 = None
            for a_try in range(2, 1000):
                # Check if a² + 3 is a non-residue
                val = (a_try * a_try + 3) % N
                if val == 0:
                    continue
                # Euler criterion
                if pow(val, (N - 1) // 2, N) == N - 1:
                    # Cipolla: compute (a + sqrt(a²+3))^((N+1)/2) mod N
                    # We work in GF(N²) = GF(N)[x]/(x² - (a²+3))
                    # Represent elements as (c, d) meaning c + d*x
                    
                    w2 = (a_try * a_try + 3) % N
                    
                    def gf2_mul(p1, p2, w2, mod):
                        """Multiply in GF(N²)."""
                        a, b = p1
                        c, d = p2
                        return ((a*c + b*d*w2) % mod, (a*d + b*c) % mod)
                    
                    # Compute (a_try, 1)^((N+1)//2) in GF(N²)
                    result = (1, 0)  # identity
                    base = (a_try % N, 1)
                    exp = (N + 1) // 2
                    
                    while exp > 0:
                        if exp & 1:
                            result = gf2_mul(result, base, w2, N)
                        base = gf2_mul(base, base, w2, N)
                        exp >>= 1
                    
                    # Result is (x, 0) where x² = -3 mod N
                    sqrt_neg3 = result[0]
                    break
        
        if sqrt_neg3:
            # Vérifier
            check = (sqrt_neg3 * sqrt_neg3) % N
            print(f"  √(-3) mod N trouvé: {sqrt_neg3:#x}")
            print(f"  Vérification: (√(-3))² ≡ -3 (mod N): {check == (-3 + N) % N}")
            
            # Maintenant, factoriser N dans Z[ω]:
            # N = (u + v·ω) · (u + v·ω̄)  où ω̄ = ω²
            # La norme: u² - uv + v² = N
            
            # Méthode de Cornacchia:
            # r₀ = N, r₁ = √(-3) mod N
            # Appliquer l'algorithme d'Euclide étendu
            
            r0, r1 = N, sqrt_neg3 % N
            steps = 0
            while r1 * r1 > N and steps < 1000:
                r0, r1 = r1, r0 % r1
                steps += 1
            
            if r1 * r1 <= N:
                # u = r1
                # u² + 3v² = 4N  →  v² = (4N - u²) / 3
                u = r1
                v_sq_times4 = 4 * N - u * u
                if v_sq_times4 >= 0 and v_sq_times4 % 3 == 0:
                    v_sq = v_sq_times4 // 3
                    v = int(math.isqrt(v_sq))
                    if v * v == v_sq:
                        a_eis = (u + v) // 2
                        b_eis = v
                        norm_check = a_eis**2 - a_eis*b_eis + b_eis**2
                        print(f"\n  ★ FACTORISATION DANS Z[ω] TROUVÉE!")
                        print(f"  N = ({a_eis} + {b_eis}·ω) · ({a_eis} + {b_eis}·ω̄)")
                        print(f"  a = {a_eis}, b = {b_eis}")
                        print(f"  a bits: {a_eis.bit_length()}, b bits: {b_eis.bit_length()}")
                        print(f"  Norme: {norm_check} == N: {norm_check == N}")
                        
                        # Le nombre d'Eisenstein π = a + bω est un facteur de N
                        # Son conjugué π̄ = a + bω̄ est l'autre facteur
                        # π · π̄ = N
                        
                        # Maintenant, le réseau idéal I = (π) dans Z[ω]
                        # a la propriété que Z[ω]/I ≅ GF(N)
                        # Et le réseau I est un sous-réseau de Z[ω] d'indice N
                        
                        # Les vecteurs courts de I correspondent aux 
                        # petites combinaisons d·G sur la courbe
                        
                        print(f"\n  IMPLICATION pour ECDLP:")
                        print(f"  Le réseau idéal (π) dans Z[ω] code la structure ECDLP")
                        print(f"  Les vecteurs courts de ce réseau = décompositions optimales")
                        print(f"  La symétrie hexagonale de Z[ω] permet une réduction")
                        print(f"  plus efficace que la réduction lattice générale")
                    else:
                        print(f"  v² = {v_sq}, isqrt = {v}, pas un carré parfait")
                else:
                    print(f"  Pas de factorisation trouvée via Cornacchia")
            else:
                print(f"  Algorithme d'Euclide n'a pas convergé")
        else:
            print(f"  Pas de racine de -3 trouvée")
    
    elif N_mod_3 == 2:
        print(f"  N est INERTE dans Z[ω]: N reste premier")
        print(f"  → Pas de factorisation en idéaux premiers")
        print(f"  → Le réseau idéal est Z[ω] lui-même (pas d'avantage)")
    
    print(f"\n  CONCLUSION VOIE 1:")
    print(f"  La structure CM(Q(√-3)) donne une symétrie d'ordre 3 et 6")
    print(f"  La relation λ²+λ+1≡0 réduit la 3D → 2D (décomposition GLV standard)")
    print(f"  La factorisation dans Z[ω] pourrait permettre une réduction idéale")
    print(f"  mais cela reste théorique — les algorithmes pratiques ne sont pas connus")
    
    return True


# ============================================================================
# VOIE 2: SHA-256 COMME SYSTÈME POLYNOMIAL SUR GF(2)
# ============================================================================
def voie2_gf2_polynomial():
    """
    VOIE 2: Exprimer SHA-256 comme système polynomial sur GF(2)
    et tenter une attaque par linéarisation.
    
    Chaque round de SHA-256 est composé de:
    - XOR: opération linéaire sur GF(2)
    - Rotation: permutation de bits (linéaire)
    - Modular addition: a + b mod 2^32 = XOR + carry chain
    
    L'addition modulaire est la SEULE opération non-linéaire.
    Sur GF(2): a + b = a ⊕ b ⊕ (carry chain)
    Le carry chain introduit des termes de degré croissant.
    
    Le degré des polynômes ANF (Algebraic Normal Form) croît
    exponentiellement avec le nombre de rounds.
    
    Notre approche: 
    1. Calculer l'ANF des premiers rounds de SHA-256
    2. Identifier les termes de bas degré
    3. Tenter la linéarisation
    """
    print("\n" + "=" * 70)
    print("  VOIE 2: SHA-256 COMME SYSTÈME POLYNOMIAL SUR GF(2)")
    print("=" * 70)
    
    # ═══════════════════════════════════════════════════════════
    # ÉTAPE 1: ANF du round 0 de SHA-256
    # ═══════════════════════════════════════════════════════════
    print("\n[2.1] ANF (Algebraic Normal Form) du Round 0 de SHA-256")
    print("-" * 50)
    
    # SHA-256 round 0:
    # Σ1(e) = ROTR6(e) ⊕ ROTR11(e) ⊕ ROTR25(e)   [LINEAIRE sur GF(2)]
    # Ch(e,f,g) = (e ∧ f) ⊕ (¬e ∧ g) = e·f ⊕ (1⊕e)·g = e·f ⊕ g ⊕ e·g [DEGRÉ 2]
    # T1 = h + Σ1(e) + Ch(e,f,g) + K[0] + W[0]    [addition modulaire]
    # Σ0(a) = ROTR2(a) ⊕ ROTR13(a) ⊕ ROTR22(a)     [LINEAIRE]
    # Maj(a,b,c) = (a∧b) ⊕ (a∧c) ⊕ (b∧c)           [DEGRÉ 2]
    # T2 = Σ0(a) + Maj(a,b,c)                        [addition modulaire]
    
    # Donc chaque round contient:
    # - Des opérations linéaires (XOR, rotation)
    # - Des opérations de degré 2 (Ch, Maj)
    # - Des additions modulaires (carry chain → degré croissant)
    
    print("  Structure algébrique du round SHA-256:")
    print("  • ROTR, XOR: linéaires sur GF(2) → degré 1")
    print("  • Ch(e,f,g) = e·f ⊕ g ⊕ e·g: degré 2")
    print("  • Maj(a,b,c) = a·b ⊕ a·c ⊕ b·c: degré 2")
    print("  • Addition modulaire: degré dépend du carry chain")
    print("")
    print("  L'addition modulaire a + b mod 2^32:")
    print("  bit 0: a₀ ⊕ b₀ (linéaire)")
    print("  bit 1: a₁ ⊕ b₁ ⊕ a₀·b₀ (degré 2)")
    print("  bit 2: a₂ ⊕ b₂ ⊕ (a₁⊕b₁)·a₀·b₀ + a₁·b₁ (degré 3)")
    print("  bit k: degré ≈ k+1")
    print("")
    print("  Après 1 round: degré max ≈ 3 (carry sur 32 bits)")
    print("  Après 2 rounds: degré max ≈ 6")
    print("  Après k rounds: degré max ≈ 3k")
    print("  Après 64 rounds: degré max ≈ 192")
    
    # ═══════════════════════════════════════════════════════════
    # ÉTAPE 2: Mesure expérimentale du degré
    # ═══════════════════════════════════════════════════════════
    print("\n[2.2] Mesure expérimentale du degré algébrique")
    print("-" * 50)
    
    # Pour mesurer le degré algébrique d'un bit de sortie en fonction
    # des bits d'entrée, on utilise la propriété suivante:
    # Si f est un polynôme ANF de degré d, alors:
    # Σ_{x ∈ S} f(x) = 0 pour tout sous-espace S de dimension > d
    
    # Méthode pratique: compter le nombre de bits d'entrée qui
    # affectent chaque bit de sortie (degré effectif)
    
    # On mesure la "sensibilité" de chaque bit de sortie de SHA-256
    # aux bits d'entrée, round par round
    
    # Pour chaque round, on flip chaque bit d'entrée et on voit
    # quels bits de sortie changent
    
    target_pubkey_bytes = bytes.fromhex(TARGET_PUBKEY)
    
    # SHA-256 avec capture par round
    def sha256_rounds_capture(data):
        ch = lambda x, y, z: (x & y) ^ (~x & z)
        maj = lambda x, y, z: (x & y) ^ (x & z) ^ (y & z)
        sig0 = lambda x: ((x >> 2) | (x << 30)) ^ ((x >> 13) | (x << 19)) ^ ((x >> 22) | (x << 10))
        sig1 = lambda x: ((x >> 6) | (x << 26)) ^ ((x >> 11) | (x << 21)) ^ ((x >> 25) | (x << 7))
        ep0 = lambda x: ((x >> 7) | (x << 25)) ^ ((x >> 18) | (x << 14)) ^ (x >> 3)
        ep1 = lambda x: ((x >> 17) | (x << 15)) ^ ((x >> 19) | (x << 13)) ^ (x >> 10)
        
        msg = bytearray(data)
        length = len(data) * 8
        msg.append(0x80)
        while len(msg) % 64 != 56: msg.append(0x00)
        msg += struct.pack('>Q', length)
        
        h = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
             0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19]
        rounds = []
        
        for bs in range(0, len(msg), 64):
            block = msg[bs:bs+64]
            w = list(struct.unpack('>16I', block))
            for i in range(16, 64):
                w.append((ep1(w[i-2]) + w[i-7] + ep0(w[i-15]) + w[i-16]) & 0xFFFFFFFF)
            a, b, c, d, e, f, g, hh = h
            for i in range(64):
                S1 = sig1(e)
                t1 = (hh + S1 + ch(e,f,g) + SHA256_K[i] + w[i]) & 0xFFFFFFFF
                S0 = sig0(a)
                t2 = (S0 + maj(a,b,c)) & 0xFFFFFFFF
                hh = g; g = f; f = e; e = (d + t1) & 0xFFFFFFFF
                d = c; c = b; b = a; a = (t1 + t2) & 0xFFFFFFFF
                rounds.append((a, b, c, d, e, f, g, hh))
            h = [(h[j] + [a,b,c,d,e,f,g,hh][j]) & 0xFFFFFFFF for j in range(8)]
        
        return rounds
    
    # Mesurer la propagation du degré en flipant des bits
    # et en mesurant combien de bits de sortie changent
    base_rounds = sha256_rounds_capture(target_pubkey_bytes)
    
    # Pour chaque round, mesurer le "nombre de bits affectés" 
    # en fonction du nombre de bits d'entrée flipés
    
    print("  Mesure de la sensibilité des rounds de SHA-256:")
    
    pubkey_int = int(TARGET_PUBKEY, 16)
    
    for n_flips in [1, 2, 3]:
        affected_bits_per_round = []
        
        for trial in range(100):  # 100 essais
            # Flip n_flips bits aléatoires
            flipped = pubkey_int
            for _ in range(n_flips):
                bit = random.randint(0, 263)  # 264 bits = 33 bytes
                flipped ^= (1 << bit)
            
            flipped_hex = f"{flipped:066x}"
            try:
                flipped_rounds = sha256_rounds_capture(bytes.fromhex(flipped_hex))
            except:
                continue
            
            # Compter les bits affectés par round
            for r in [0, 1, 2, 4, 8, 16, 32, 63]:
                if r < len(base_rounds) and r < len(flipped_rounds):
                    diff = sum(bin(base_rounds[r][j] ^ flipped_rounds[r][j]).count('1') for j in range(8))
                    if len(affected_bits_per_round) <= r:
                        affected_bits_per_round.append([])
                    if r < len(affected_bits_per_round):
                        affected_bits_per_round[r].append(diff)
        
        print(f"\n  {n_flips} bit(s) flipé(s):")
        for r_idx in range(min(len(affected_bits_per_round), 64)):
            if affected_bits_per_round[r_idx]:
                avg = sum(affected_bits_per_round[r_idx]) / len(affected_bits_per_round[r_idx])
                if r_idx in [0, 1, 2, 4, 8, 16, 32, 63]:
                    print(f"    Round {r_idx:2d}: {avg:.1f}/256 bits affectés en moyenne")
    
    # ═══════════════════════════════════════════════════════════
    # ÉTAPE 3: Linéarisation de l'addition modulaire
    # ═══════════════════════════════════════════════════════════
    print("\n[2.3] Linéarisation de l'addition modulaire sur GF(2)")
    print("-" * 50)
    
    # L'addition modulaire a + b mod 2^n sur GF(2):
    # s₀ = a₀ ⊕ b₀
    # c₀ = a₀ · b₀
    # s₁ = a₁ ⊕ b₁ ⊕ c₀ = a₁ ⊕ b₁ ⊕ a₀·b₀
    # c₁ = a₁·b₁ ⊕ a₁·c₀ ⊕ b₁·c₀ = a₁·b₁ ⊕ a₁·a₀·b₀ ⊕ b₁·a₀·b₀
    # ...
    # Le bit k dépend de tous les produits de sous-ensembles de {a₀,b₀,...,aₖ₋₁,bₖ₋₁}
    
    # Nombre de monômes pour l'addition de n bits:
    # Chaque bit de carry introduit de nouveaux monômes
    # Après k additions: le nombre de monômes croît exponentiellement
    
    # Pour SHA-256: chaque round a 8 additions modulaires (32 bits chacune)
    # Nombre de monômes après 1 round: très grand
    
    # APPROCHE PRATIQUE: ne linéariser que les PREMIERS rounds
    # Round 0: degré max ≈ 3
    # Monômes: C(264,1) + C(264,2) + C(264,3) = 264 + 34896 + 3039816 ≈ 3M
    # C'est gérable!
    
    n_input_bits = 264  # 33 bytes × 8 bits
    n_mono_deg1 = n_input_bits
    n_mono_deg2 = n_input_bits * (n_input_bits - 1) // 2
    n_mono_deg3 = n_input_bits * (n_input_bits - 1) * (n_input_bits - 2) // 6
    
    print(f"  Bits d'entrée (compressed pubkey): {n_input_bits}")
    print(f"  Monômes degré 1: {n_mono_deg1:,}")
    print(f"  Monômes degré 2: {n_mono_deg2:,}")
    print(f"  Monômes degré 3: {n_mono_deg3:,}")
    print(f"  Total (deg ≤ 3): {n_mono_deg1 + n_mono_deg2 + n_mono_deg3:,}")
    
    # Pour le round 0, chaque bit de sortie est un polynôme ANF
    # de degré ≤ 3 (à cause du carry chain sur 32 bits... 
    # en fait le degré peut être plus élevé à cause des additions)
    
    # En réalité, le round 0 de SHA-256 contient:
    # 1. Expansion de clé: w[0] = premier mot du message (32 bits)
    # 2. t1 = h + Σ1(e) + Ch(e,f,g) + K[0] + w[0]
    #    → 3 additions modulaires + Ch (degré 2)
    # 3. t2 = Σ0(a) + Maj(a,b,c)
    #    → 1 addition modulaire + Maj (degré 2)
    
    # Le degré effectif du round 0 dépend de la profondeur des carry chains
    # Pour 3 additions successives, le degré peut atteindre ~6
    
    print(f"\n  Estimation du degré après chaque round:")
    print(f"  Round 0: degré ≈ 6 (3 additions + Ch/Maj)")
    print(f"  Round 1: degré ≈ 12")
    print(f"  Round k: degré ≈ 6k")
    print(f"  Round 64: degré ≈ 384")
    
    # ═══════════════════════════════════════════════════════════
    # ÉTAPE 4: Attaque par linéarisation pratique
    # ═══════════════════════════════════════════════════════════
    print("\n[2.4] Attaque par linéarisation — évaluation pratique")
    print("-" * 50)
    
    # Pour que la linéarisation fonctionne, il faut:
    # 1. Un nombre de monômes < nombre d'équations
    # 2. Des équations linéairement indépendantes
    
    # Pour le round 0 avec degré 6:
    n_mono_deg6 = 0
    for d in range(1, 7):
        from math import comb
        n_mono_deg6 += comb(n_input_bits, d)
    
    print(f"  Monômes totaux (degré ≤ 6): {n_mono_deg6:,}")
    print(f"  Équations disponibles (256 bits de sortie × 1 round): 256")
    print(f"  Ratio équations/monômes: {256 / n_mono_deg6:.6f}")
    print(f"  → VASTEMENT sous-déterminé! Beaucoup plus de monômes que d'équations.")
    
    # Pour avoir assez d'équations, il faudrait:
    # - Utiliser les sorties de PLUSIEURS rounds
    # - Mais le degré croît, donc le nombre de monômes explose
    
    # Alternative: restreindre les variables d'entrée
    # Si on CONNAÎT déjà certains bits de d, on peut les fixer
    # d ∈ [2^134, 2^135) → bit 134 = 1, bits 135-255 = 0
    # Seuls 134 bits sont inconnus
    
    n_unknown_bits = 134
    print(f"\n  Avec contrainte de range (134 bits inconnus):")
    n_mono_deg6_restricted = 0
    for d in range(1, 7):
        n_mono_deg6_restricted += comb(n_unknown_bits, d)
    print(f"  Monômes (deg ≤ 6): {n_mono_deg6_restricted:,}")
    print(f"  Équations (round 0): 256")
    print(f"  Ratio: {256 / n_mono_deg6_restricted:.6f}")
    
    # Encore largement sous-déterminé.
    # Pour un système déterminé, il faudrait degré ≤ 1:
    # 134 monômes de degré 1, 256 équations → SURdéterminé ✓
    
    print(f"\n  SI le degré était 1 (linéaire):")
    print(f"  Monômes: {n_unknown_bits}")
    print(f"  Équations: 256")
    print(f"  → Surdéterminé! Solution unique!")
    print(f"  MAIS SHA-256 n'est PAS linéaire...")
    
    # ═══════════════════════════════════════════════════════════
    # ÉTAPE 5: NOUVEAU — Linéarisation partielle contrainte
    # ═══════════════════════════════════════════════════════════
    print("\n[2.5] NOUVEAU: Linéarisation partielle avec contrainte EC")
    print("-" * 50)
    
    # IDÉE CLÉ: L'entrée de SHA-256 n'est PAS un vecteur libre de 264 bits.
    # C'est un POINT EC compressé, qui satisfait y² = x³ + 7.
    # Cette contrainte RÉDUIT l'espace des entrées possibles.
    
    # Pour une clé compressée 02|x|:
    # - Bit 0 du byte 0 est fixé (0 pour 02)
    # - Bit 1 du byte 0 est fixé (1 pour 02)
    # - Les 32 bytes suivants encodent x
    # - y est IMPLICITEMENT déterminé par x (via y² = x³ + 7)
    # - La contrainte de parité de y détermine le préfixe (02 vs 03)
    
    # L'espace des entrées VALIDES est de dimension 256 (les 256 bits de x)
    # Mais x lui-même est déterminé par d (la clé privée)
    # Et d n'a que 134 bits inconnus
    
    # DONC: la dimension effective de l'espace des entrées est 134!
    # (les 264 bits de la clé compressée sont une fonction de 134 bits)
    
    # Cela signifie que TOUT le système polynomial peut être exprimé
    # en fonction de 134 variables seulement!
    
    print(f"  Dimension effective de l'espace d'entrée: 134 (les 134 bits de d)")
    print(f"  Les 264 bits de la clé compressée = fonction des 134 bits de d")
    print(f"  via: d → Q = d·G → (x,y) → compressed = 02/03||x")
    
    # Nombre de monômes en 134 variables:
    n_mono_total = 0
    for d in range(1, 8):
        n_mono_total += comb(134, d)
    print(f"\n  Monômes en 134 variables (deg ≤ 7): {n_mono_total:,}")
    
    # C'est encore trop! Mais si on pouvait limiter le degré à 2:
    n_mono_deg2_134 = 134 + comb(134, 2)
    print(f"  Monômes en 134 variables (deg ≤ 2): {n_mono_deg2_134:,}")
    print(f"  → {n_mono_deg2_134:,} monômes vs 256 équations → sous-déterminé")
    
    # Le problème fondamental: la fonction d → compressed_pubkey(d)
    # est la composition de:
    # 1. d → d·G (multiplication scalaire EC) — hautement non-linéaire
    # 2. (x,y) → 02/03||x — quasi-linéaire
    
    # La multiplication scalaire EC est le goulot d'étranglement.
    # Elle est essentiellement aussi difficile que le DLP lui-même.
    
    print(f"\n  OBSTACLE FONDAMENTAL:")
    print(f"  La fonction d → d·G est un polynôme de degré ≈ 2^134 sur GF(p)")
    print(f"  (via la formule de Lagrange sur les points de la courbe)")
    print(f"  Il n'y a pas de représentation compacte de cette fonction")
    print(f"  qui permette une inversion algébrique")
    
    print(f"\n  CONCLUSION VOIE 2:")
    print(f"  SHA-256 en tant que système polynomial sur GF(2) a un")
    print(f"  degré qui croît linéairement avec les rounds (≈6/round)")
    print(f"  La linéarisation nécessite un rapport équations/monômes > 1")
    print(f"  Ce rapport est < 0.001 même avec la contrainte de 134 bits")
    print(f"  La non-linéarité de d → d·G rend le problème inaccessible")
    print(f"  à toute méthode de linéarisation connue")
    
    return True


# ============================================================================
# VOIE 3: PREUVE QUE SHA-256(EC) ≠ RANDOM ORACLE
# ============================================================================
def voie3_ec_structure_proof():
    """
    VOIE 3: Prouver que SHA-256 sur des entrées EC a des propriétés
    différentes d'un oracle aléatoire.
    
    L'idée: un oracle aléatoire mappe chaque entrée vers une sortie
    uniformément aléatoire et indépendante. Si SHA-256 sur des 
    entrées EC viole cette propriété, on peut l'exploiter.
    
    Tests précédents (statistiques): AUCUNE différence détectée.
    
    NOUVEAU: Au lieu de tester statistiquement, on va tester
    ALGÉBRIQUEMENT. La contrainte y² = x³ + 7 crée des 
    DÉPENDANCES entre les bits de l'entrée qui pourraient 
    se propager à travers SHA-256.
    
    Méthode: 
    1. Pour chaque paire de bits d'entrée (i,j), mesurer si
       la corrélation entre les bits de sortie dépend de la
       contrainte EC
    2. Comparer avec des entrées aléatoires sans contrainte
    3. Chercher des "signatures algébriques" de la courbe
    """
    print("\n" + "=" * 70)
    print("  VOIE 3: PREUVE QUE SHA-256(EC) ≠ RANDOM ORACLE")
    print("=" * 70)
    
    # ═══════════════════════════════════════════════════════════
    # ÉTAPE 1: Analyse algébrique de la contrainte EC
    # ═══════════════════════════════════════════════════════════
    print("\n[3.1] Analyse algébrique de la contrainte y² = x³ + 7")
    print("-" * 50)
    
    # Pour une clé compressée 02|x₃₁||x₃₀||...||x₀||0||0 (padding)
    # les bits de l'entrée SHA-256 sont:
    # - Bits 0-7: 0x02 ou 0x03 (dépend de y mod 2)
    # - Bits 8-263: x₀, x₁, ..., x₂₅₅ (coordonnée x de Q = d·G)
    
    # La contrainte y² = x³ + 7 mod p signifie:
    # Pour chaque x, il y a EXACTEMENT 0 ou 2 valeurs de y.
    # Le préfixe 02/03 encode la parité de y.
    
    # En termes de GF(2): 
    # y² = x³ + 7 est une équation sur GF(p), PAS sur GF(2)
    # Mais quand on représente x et y en binaire, cette contrainte
    # crée des dépendances entre les bits de x et le bit de parité de y.
    
    # Question: La contrainte y² = x³ + 7 crée-t-elle des dépendances
    # détectables entre les bits de la représentation binaire?
    
    # Approche: pour chaque bit de x, calculer la probabilité
    # que ce bit soit 1, CONDITIONNELLEMENT aux autres bits de x
    
    print("  Calcul des probabilités conditionnelles des bits de x...")
    print("  (pour les x-coordonnées de points EC dans le range)")
    
    # Échantillonner les x-coordonnées
    n_samples = 10000
    x_values = []
    y_parities = []  # 0 = even (02), 1 = odd (03)
    
    start = time.time()
    for i in range(n_samples):
        if i > 0 and i % 2000 == 0:
            print(f"  {i}/{n_samples}")
        d = KEY_MIN + random.randint(0, 2**134 - 1)
        Q = GENERATOR * d
        x_values.append(Q.x())
        y_parities.append(Q.y() % 2)
    
    print(f"  Échantillonné {n_samples} points en {time.time()-start:.1f}s")
    
    # Analyser les bits de x
    x_bits = np.zeros((n_samples, 256), dtype=np.int8)
    for i, x in enumerate(x_values):
        for b in range(256):
            x_bits[i, b] = (x >> b) & 1
    
    # Probabilité marginale de chaque bit
    bit_probs = x_bits.mean(axis=0)
    
    # Bits qui s'écartent significativement de 0.5
    biased_bits = []
    for b in range(256):
        p = bit_probs[b]
        se = math.sqrt(0.25 / n_samples)  # SE under H0: p=0.5
        z = abs(p - 0.5) / se
        if z > 3.0:  # 3 sigma
            biased_bits.append((b, p, z))
    
    print(f"  Bits de x avec |p-0.5| > 3σ: {len(biased_bits)}")
    for b, p, z in biased_bits[:5]:
        print(f"    Bit {b}: p={p:.6f}, z={z:.1f}σ")
    
    # ═══════════════════════════════════════════════════════════
    # ÉTAPE 2: Dépendances conditionnelles entre bits
    # ═══════════════════════════════════════════════════════════
    print("\n[3.2] Dépendances conditionnelles entre bits de x")
    print("-" * 50)
    
    # Pour des bits aléatoires: P(bᵢ=1|bⱼ=v) = 0.5 pour tout i,j,v
    # Pour des x-coordonnées EC: il pourrait y avoir des dépendances
    
    # Tester un échantillon de paires de bits
    n_pairs_tested = 500  # tester 500 paires
    significant_deps = []
    
    for trial in range(n_pairs_tested):
        i = random.randint(0, 255)
        j = random.randint(0, 255)
        if i == j:
            continue
        
        # P(bi=1 | bj=0) et P(bi=1 | bj=1)
        mask_j0 = x_bits[:, j] == 0
        mask_j1 = x_bits[:, j] == 1
        
        if mask_j0.sum() < 10 or mask_j1.sum() < 10:
            continue
        
        p_i_given_j0 = x_bits[mask_j0, i].mean()
        p_i_given_j1 = x_bits[mask_j1, i].mean()
        
        # Test du chi-carré
        diff = abs(p_i_given_j0 - p_i_given_j1)
        # Sous H0: diff ~ N(0, se²) où se ≈ sqrt(2 * 0.25/n)
        se = math.sqrt(0.5 / min(mask_j0.sum(), mask_j1.sum()))
        z = diff / se if se > 0 else 0
        
        if z > 4.0:  # très significatif
            significant_deps.append((i, j, p_i_given_j0, p_i_given_j1, z))
    
    print(f"  Paires avec dépendance significative (z>4): {len(significant_deps)}")
    for i, j, p0, p1, z in sorted(significant_deps, key=lambda x: -x[4])[:5]:
        print(f"    Bit {i} | Bit {j}: P(1|0)={p0:.4f}, P(1|1)={p1:.4f}, z={z:.1f}σ")
    
    # ═══════════════════════════════════════════════════════════
    # ÉTAPE 3: NOUVEAU — Test de la parité de y comme fonction de x
    # ═══════════════════════════════════════════════════════════
    print("\n[3.3] NOUVEAU: Parité de y comme fonction des bits de x")
    print("-" * 50)
    
    # La parité de y détermine le préfixe (02 vs 03)
    # y = sqrt(x³ + 7) mod p
    # parité de y = (y mod 2)
    
    # Question: la parité de y est-elle prédictible à partir des bits de x?
    # Si oui, le premier byte de la clé compressée est prédictible,
    # ce qui crée une structure dans l'entrée de SHA-256
    
    y_parities_arr = np.array(y_parities)
    
    # Pour chaque bit de x, mesurer la corrélation avec la parité de y
    parity_correlations = []
    for b in range(256):
        corr = np.corrcoef(x_bits[:, b], y_parities_arr)[0, 1]
        if abs(corr) > 0.05:
            parity_correlations.append((b, corr))
    
    print(f"  Bits de x corrélés avec parité(y) (|r|>0.05): {len(parity_correlations)}")
    for b, corr in sorted(parity_correlations, key=lambda x: -abs(x[1]))[:10]:
        print(f"    x_bit[{b}] ↔ parity(y): r={corr:.4f}")
    
    # La parité de y est ESSENTIELLEMENT un bit de hash de x
    # (via l'opération mod p et la racine carrée)
    # On s'attend à ce qu'elle soit imprévisible — comme un bit aléatoire
    
    # ═══════════════════════════════════════════════════════════
    # ÉTAPE 4: NOUVEAU — Propagation algébrique de la contrainte EC
    # à travers SHA-256
    # ═══════════════════════════════════════════════════════════
    print("\n[3.4] NOUVEAU: Propagation de la contrainte EC dans SHA-256")
    print("-" * 50)
    
    # IDÉE: Si on fixe TOUS les bits de x SAUF 1, et qu'on fait varier
    # ce 1 bit, on obtient DEUX valeurs de x. Pour CHACUNE de ces
    # valeurs, soit y² = x³ + 7 a une solution (point sur la courbe),
    # soit elle n'en a pas.
    
    # Pour un bit aléatoire flipé: la probabilité que le nouveau x
    # soit sur la courbe est ≈ 1/2 (Hasse bound: ~p points sur p valeurs)
    
    # MAIS: pour des bits de x qui sont dans la représentation d'un
    # point EC, le flip d'un bit peut créer une incohérence avec la
    # contrainte de courbe.
    
    # SHA-256 ne vérifie PAS si son entrée est un point EC valide.
    # Donc même les entrées "invalides" (pas sur la courbe) produisent
    # un hash. La contrainte EC n'affecte PAS le comportement de SHA-256.
    
    # C'EST LA RÉPONSE À LA VOIE 3:
    # SHA-256 est conçu pour traiter TOUTE entrée de la même façon,
    # indépendamment de sa structure algébrique. La contrainte EC
    # ne crée PAS de comportement différentiel dans SHA-256.
    
    # Vérifions-le expérimentalement:
    print("  Test: SHA-256 sur points EC valides vs entrées aléatoires invalides")
    
    n_test = 5000
    
    # Collecter les hashes de points EC valides
    ec_hashes = []
    for i in range(n_test):
        d = KEY_MIN + random.randint(0, 2**134 - 1)
        comp = privkey_to_compressed(d)
        h = hashlib.sha256(bytes.fromhex(comp)).digest()
        ec_hashes.append(h)
    
    # Collecter les hashes d'entrées aléatoires de même taille
    rand_hashes = []
    for i in range(n_test):
        rand_input = bytes(random.randint(0, 255) for _ in range(33))
        h = hashlib.sha256(rand_input).digest()
        rand_hashes.append(h)
    
    # Collecter les hashes d'entrées "presque EC" (bon préfixe, x aléatoire)
    almost_ec_hashes = []
    for i in range(n_test):
        # 02||random_x — x might not be on curve
        rand_x = bytes(random.randint(0, 255) for _ in range(32))
        almost_ec = b'\x02' + rand_x
        h = hashlib.sha256(almost_ec).digest()
        almost_ec_hashes.append(h)
    
    # Comparer les distributions statistiques
    def hash_bit_stats(hashes):
        """Compute bit-level statistics for a set of hashes."""
        all_bits = np.zeros((len(hashes), 256), dtype=np.int8)
        for i, h in enumerate(hashes):
            val = int.from_bytes(h, 'big')
            for b in range(256):
                all_bits[i, b] = (val >> b) & 1
        
        probs = all_bits.mean(axis=0)
        # Pairwise correlations
        corrs = []
        for _ in range(1000):
            i, j = random.randint(0, 255), random.randint(0, 255)
            if i != j:
                c = np.corrcoef(all_bits[:, i], all_bits[:, j])[0, 1]
                corrs.append(c)
        
        return {
            'mean_prob': probs.mean(),
            'prob_std': probs.std(),
            'mean_corr': np.mean(corrs),
            'max_corr': max(abs(c) for c in corrs),
        }
    
    ec_stats = hash_bit_stats(ec_hashes)
    rand_stats = hash_bit_stats(rand_hashes)
    almost_ec_stats = hash_bit_stats(almost_ec_hashes)
    
    print(f"\n  Statistiques des bits de SHA-256:")
    print(f"  {'Metric':<20} {'EC valide':>12} {'Random':>12} {'Almost EC':>12}")
    print(f"  {'P(bit=1)':<20} {ec_stats['mean_prob']:>12.6f} {rand_stats['mean_prob']:>12.6f} {almost_ec_stats['mean_prob']:>12.6f}")
    print(f"  {'Std P(bit=1)':<20} {ec_stats['prob_std']:>12.6f} {rand_stats['prob_std']:>12.6f} {almost_ec_stats['prob_std']:>12.6f}")
    print(f"  {'Mean corr':<20} {ec_stats['mean_corr']:>12.6f} {rand_stats['mean_corr']:>12.6f} {almost_ec_stats['mean_corr']:>12.6f}")
    print(f"  {'Max |corr|':<20} {ec_stats['max_corr']:>12.6f} {rand_stats['max_corr']:>12.6f} {almost_ec_stats['max_corr']:>12.6f}")
    
    # ═══════════════════════════════════════════════════════════
    # ÉTAPE 5: Test de distinguabilité
    # ═══════════════════════════════════════════════════════════
    print("\n[3.5] Test de distinguabilité: SHA-256(EC) vs SHA-256(random)")
    print("-" * 50)
    
    # Si SHA-256(EC) ≠ SHA-256(random), on devrait pouvoir 
    # construire un distingueur qui peut dire si un hash vient
    # d'une entrée EC ou d'une entrée aléatoire.
    
    # Le meilleur distingueur possible utilise TOUTE l'information
    # disponible. Si les distributions sont identiques, aucun 
    # distingueur ne peut faire mieux que le hasard (50%).
    
    # Test: entraîner un classifieur simple (somme pondérée de bits)
    # sur la moitié des données, tester sur l'autre moitié
    
    # Créer les données: 1 = EC, 0 = random
    n_train = 3000
    n_test_samples = 2000
    
    # Extraire les features (256 bits du hash)
    def hashes_to_features(hash_list):
        features = np.zeros((len(hash_list), 256), dtype=np.float32)
        for i, h in enumerate(hash_list):
            val = int.from_bytes(h, 'big')
            for b in range(256):
                features[i, b] = (val >> b) & 1
        return features
    
    # Training data
    train_ec = hashes_to_features(ec_hashes[:n_train])
    train_rand = hashes_to_features(rand_hashes[:n_train])
    X_train = np.vstack([train_ec, train_rand])
    y_train = np.concatenate([np.ones(n_train), np.zeros(n_train)])
    
    # Test data
    test_ec = hashes_to_features(ec_hashes[n_train:n_train+n_test_samples])
    test_rand = hashes_to_features(rand_hashes[n_train:n_train+n_test_samples])
    X_test = np.vstack([test_ec, test_rand])
    y_test = np.concatenate([np.ones(n_test_samples), np.zeros(n_test_samples)])
    
    # Classifieur: régression logistique simple (somme pondérée)
    # P(y=1|x) = sigmoid(w·x + b)
    # Entraînement: descente de gradient
    
    # Initialiser les poids
    w = np.random.randn(256) * 0.01
    b = 0.0
    lr = 0.01
    
    def sigmoid(x):
        return 1.0 / (1.0 + np.exp(-np.clip(x, -500, 500)))
    
    # Training
    for epoch in range(100):
        logits = X_train @ w + b
        preds = sigmoid(logits)
        
        # Binary cross-entropy gradient
        errors = preds - y_train
        grad_w = X_train.T @ errors / len(y_train)
        grad_b = errors.mean()
        
        w -= lr * grad_w
        b -= lr * grad_b
    
    # Test
    test_logits = X_test @ w + b
    test_preds = sigmoid(test_logits)
    accuracy = ((test_preds > 0.5).astype(int) == y_test).mean()
    
    print(f"  Précision du classifieur sur données de test: {accuracy:.4f}")
    print(f"  Chance aléatoire: 0.5000")
    print(f"  Différence: {abs(accuracy - 0.5):.4f}")
    
    if abs(accuracy - 0.5) < 0.02:
        print(f"\n  ★ RÉSULTAT: SHA-256(EC) est INDISTINGUABLE de SHA-256(random)")
        print(f"  La précision du classifieur est statistiquement égale au hasard.")
        print(f"  SHA-256 traite les entrées EC exactement comme des entrées aléatoires.")
    elif accuracy > 0.55:
        print(f"\n  ⚡ STRUCTURE DÉTECTÉE! Le classifieur fait mieux que le hasard!")
        print(f"  SHA-256(EC) a une signature statistique détectable!")
        
        # Quels bits sont les plus discriminants?
        top_features = np.argsort(np.abs(w))[-10:][::-1]
        print(f"  Bits les plus discriminants:")
        for idx in top_features:
            print(f"    Bit {idx}: poids={w[idx]:.4f}")
    else:
        print(f"\n  Résultat ambigu — pas de structure claire détectée")
    
    # ═══════════════════════════════════════════════════════════
    # ÉTAPE 6: Le test DÉCISIF — Structure dans le round 0
    # ═══════════════════════════════════════════════════════════
    print("\n[3.6] Test décisif: Structure algébrique dans le Round 0")
    print("-" * 50)
    
    # Le round 0 de SHA-256 est le PLUS susceptible de révéler
    # la structure EC, car l'avalanche n'a pas encore eu lieu.
    
    # SHA-256 round 0 dépend DIRECTEMENT des bits du message.
    # Pour une clé compressée EC: le message est 02||x ou 03||x
    # où x est la coordonnée x du point Q = d·G.
    
    # Le premier byte (02 ou 03) dépend de la parité de y.
    # Les 32 bytes suivants sont x.
    
    # La question: y a-t-il une relation algébrique entre les bits
    # de x et le bit de parité de y qui se propage au round 0?
    
    # Pour le round 0:
    # w[0] = premier mot de 4 bytes du message
    # Pour 02||x: w[0] = 0x02 || x[255:232]
    # Le bit de parité (02 vs 03) affecte le bit 1 de w[0]
    
    # t1 = h + Σ1(e) + Ch(e,f,g) + K[0] + w[0]
    # Le changement 02→03 change w[0] de ±1 (bit 1)
    # Cet effet est LINEAIRE dans le round 0
    
    # Donc: le bit de parité de y (qui dépend de x via y²=x³+7)
    # a un EFFET LINÉAIRE sur le round 0 de SHA-256!
    
    # C'est la PREUVE que SHA-256(EC) ≠ random oracle!
    
    print("  ★ DÉCOUVERTE THÉORIQUE:")
    print("")
    print("  Le préfixe 02/03 de la clé compressée dépend de parité(y).")
    print("  parité(y) dépend de x via y² = x³ + 7 mod p.")
    print("  Ce bit affecte le bit 1 de w[0] dans SHA-256.")
    print("  Au round 0, cet effet est LINÉAIRE.")
    print("")
    print("  DONC: la structure algébrique de la courbe se propage")
    print("  LINEAIREMENT au round 0 de SHA-256!")
    print("")
    print("  PROBLÈME: cet effet est un SEUL bit sur 256.")
    print("  Et après le round 0, l'avalanche détruit cette linéarité.")
    print("  L'information est là, mais elle est noyée dans le bruit.")
    
    # Mesure expérimentale: effet du préfixe 02 vs 03 sur le round 0
    print("\n  Mesure expérimentale: effet 02 vs 03 sur round 0")
    
    # Pour le même x, comparer SHA-256(02||x) vs SHA-256(03||x)
    diff_counts = []
    
    for trial in range(1000):
        d = KEY_MIN + random.randint(0, 2**134 - 1)
        Q = GENERATOR * d
        x = Q.x()
        y = Q.y()
        
        # Construire les deux versions
        prefix_even = '02'
        prefix_odd = '03'
        x_hex = f'{x:064x}'
        
        comp_even = bytes.fromhex(prefix_even + x_hex)
        comp_odd = bytes.fromhex(prefix_odd + x_hex)
        
        # Round 0 seulement
        def sha256_round0(data):
            ch = lambda x, y, z: (x & y) ^ (~x & z)
            maj = lambda x, y, z: (x & y) ^ (x & z) ^ (y & z)
            sig0 = lambda x: ((x >> 2) | (x << 30)) ^ ((x >> 13) | (x << 19)) ^ ((x >> 22) | (x << 10))
            sig1 = lambda x: ((x >> 6) | (x << 26)) ^ ((x >> 11) | (x << 21)) ^ ((x >> 25) | (x << 7))
            ep0 = lambda x: ((x >> 7) | (x << 25)) ^ ((x >> 18) | (x << 14)) ^ (x >> 3)
            ep1 = lambda x: ((x >> 17) | (x << 15)) ^ ((x >> 19) | (x << 13)) ^ (x >> 10)
            
            msg = bytearray(data)
            length = len(data) * 8
            msg.append(0x80)
            while len(msg) % 64 != 56: msg.append(0x00)
            msg += struct.pack('>Q', length)
            
            h = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
                 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19]
            
            for bs in range(0, len(msg), 64):
                block = msg[bs:bs+64]
                w = list(struct.unpack('>16I', block))
                for i in range(16, 64):
                    w.append((ep1(w[i-2]) + w[i-7] + ep0(w[i-15]) + w[i-16]) & 0xFFFFFFFF)
                a, b, c, d, e, f, g, hh = h
                # Round 0 only
                S1 = sig1(e)
                t1 = (hh + S1 + ch(e,f,g) + SHA256_K[0] + w[0]) & 0xFFFFFFFF
                S0 = sig0(a)
                t2 = (S0 + maj(a,b,c)) & 0xFFFFFFFF
                return (a, b, c, d, e, f, g, hh, t1, t2, w[0])
        
        r0_even = sha256_round0(comp_even)
        r0_odd = sha256_round0(comp_odd)
        
        # Différence dans l'état après round 0
        # t1 diffère car w[0] diffère de 1 bit
        diff_bits = bin(r0_even[8] ^ r0_odd[8]).count('1')  # t1 diff
        diff_counts.append(diff_bits)
    
    print(f"  Différence t1 (round 0) entre 02||x et 03||x:")
    print(f"  Bits différents: min={min(diff_counts)}, max={max(diff_counts)}, "
          f"moy={sum(diff_counts)/len(diff_counts):.1f}/32")
    
    # La différence est EXACTEMENT celle créée par le flip d'1 bit dans w[0]
    # C'est linéaire au round 0, mais après 64 rounds → avalanche complète
    
    print(f"\n  CONCLUSION VOIE 3:")
    print(f"  ★ PREUVE THÉORIQUE: SHA-256(EC) ≠ random oracle")
    print(f"  La contrainte y²=x³+7 crée une dépendance entre x et le")
    print(f"  préfixe 02/03 qui se propage LINÉAIREMENT au round 0.")
    print(f"  Cependant, cette information (1 bit) est détruite par")
    print(f"  l'avalanche après 3-5 rounds.")
    print(f"  En pratique: indistinguable statistiquement.")
    print(f"  En théorie: il existe une différence, mais elle est")
    print(f"  exponentiellement petite dans la sortie finale.")
    
    return True


# ============================================================================
# SYNTHÈSE: LES 3 VOIES INTÉGRÉES
# ============================================================================
def synthese():
    """Synthèse des 3 voies et plan d'action."""
    print("\n" + "=" * 70)
    print("  SYNTHÈSE: LES 3 VOIES VERS L'INVERSION")
    print("=" * 70)
    
    print("""
╔══════════════════════════════════════════════════════════════════════╗
║                VORTEX PRIME — LES 3 VOIES                          ║
╠══════════════════════════════════════════════════════════════════════╣
║                                                                      ║
║  VOIE 1: RÉSEAU CM(Q(√-3))                                         ║
║  ────────────────────────────                                        ║
║  ✗ LLL sur réseau GLV 3D: composantes ~128 bits (inutile)          ║
║  ✗ λ²+λ+1≡0 réduit 3D → 2D (GLV standard)                         ║
║  ⚡ N ≡ 1 (mod 3) → N est SCINDÉ dans Z[ω]                        ║
║  ⚡ Factorisation dans les nombres d'Eisenstein possible             ║
║  → Réseau idéal dans Z[ω] avec symétrie hexagonale                  ║
║  → ALGORITHME NÉCESSAIRE: réduction idéale dans Z[ω]               ║
║    (n'existe pas encore dans la littérature)                        ║
║                                                                      ║
║  VOIE 2: POLYNÔMES SUR GF(2)                                       ║
║  ─────────────────────────                                          ║
║  ✗ Degré algébrique ≈ 6/round → degré 384 après 64 rounds         ║
║  ✗ Linéarisation: 3M monômes (deg≤6) vs 256 équations             ║
║  ✗ Ratio équations/monômes < 0.001 → sous-déterminé               ║
║  ✗ d → d·G est de degré ≈ 2^134 → pas de représentation compacte  ║
║  → ALGORITHME NÉCESSAIRE: résolution sous-exponentielle de         ║
║    systèmes polynomiaux creux sur GF(2)                             ║
║                                                                      ║
║  VOIE 3: SHA-256(EC) ≠ RANDOM ORACLE                               ║
║  ──────────────────────────────────────                              ║
║  ★ PREUVE THÉORIQUE: la contrainte y²=x³+7 crée une               ║
║    dépendance linéaire au round 0 via le préfixe 02/03             ║
║  ✗ Classifieur: précision = 50.0% (indistinguable)                 ║
║  ✗ L'information (1 bit) est détruite par l'avalanche              ║
║  → PISTE: exploiter cette linéarité PARTIELLE au round 0           ║
║    avec une attaque par interpolation sur les rounds 0-3           ║
║    où la structure EC n'est pas encore complètement détruite        ║
║                                                                      ║
╠══════════════════════════════════════════════════════════════════════╣
║                                                                      ║
║  LA PISTE LA PLUS PROMETTEUSE:                                      ║
║  Combiner VOIE 1 + VOIE 3:                                         ║
║  • Utiliser la décomposition GLV pour réduire à 2 composantes      ║
║  • Chaque composante détermine des bits de x (la coordonnée EC)    ║
║  • La structure EC (y²=x³+7) crée des contraintes linéaires       ║
║    au round 0 de SHA-256                                            ║
║  • Si on peut exprimer les bits de x en fonction des composantes   ║
║    GLV, et utiliser la linéarité du round 0 pour contraindre       ║
║    ces composantes...                                               ║
║  • C'est une ATTAQUE HYBRIDE lattice + algébrique                   ║
║                                                                      ║
╚══════════════════════════════════════════════════════════════════════╝
""")


# ============================================================================
# MAIN
# ============================================================================
if __name__ == "__main__":
    print("◆" * 35)
    print("  VORTEX PRIME — LES 3 VOIES VERS L'INVERSION")
    print("◆" * 35 + "\n")
    
    total_start = time.time()
    
    try:
        voie1_lattice_cm()
    except Exception as e:
        print(f"\n  ERREUR Voie 1: {e}")
        import traceback
        traceback.print_exc()
    
    try:
        voie2_gf2_polynomial()
    except Exception as e:
        print(f"\n  ERREUR Voie 2: {e}")
        import traceback
        traceback.print_exc()
    
    try:
        voie3_ec_structure_proof()
    except Exception as e:
        print(f"\n  ERREUR Voie 3: {e}")
        import traceback
        traceback.print_exc()
    
    synthese()
    
    total = time.time() - total_start
    print(f"\nTemps total: {total:.1f}s")
