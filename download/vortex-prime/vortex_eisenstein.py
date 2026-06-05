#!/usr/bin/env python3
"""
VORTEX PRIME — Factorisation de N dans Z[ω] + Attaque Hybride
==============================================================

DÉCOUVERTES PRÉCÉDENTES:
- N ≡ 1 (mod 3) → N est SCINDÉ dans Z[ω]
- √(-3) mod N trouvé
- λ² + λ + 1 ≡ 0 (mod N) vérifié
- La contrainte EC se propage LINÉAIREMENT au round 0 de SHA-256

MAINTENANT:
1. Factoriser N dans Z[ω] via l'algorithme de Cornacchia
2. Utiliser la factorisation pour construire un réseau idéal
3. Explorer l'attaque hybride lattice + algébrique
"""

import math
import time
import random
import hashlib
from ecdsa import SECP256k1, SigningKey
from ecdsa.numbertheory import inverse_mod
from ecdsa.ellipticcurve import Point

ORDER = SECP256k1.order
GENERATOR = SECP256k1.generator
CURVE = SECP256k1.curve
P = CURVE.p()
N = ORDER

LAMBDA = 0x5363ad4cc05c30e0a5261c028812645a122e22ea20816678df02967c1b23bd72
LAMBDA2 = pow(LAMBDA, 2, N)
BETA = 0x7ae96a2b657c07106e64479eac3434e99cf0497512f58995c1396c28719501ee


def factor_N_in_eisenstein():
    """
    Factoriser N (l'ordre de secp256k1) dans l'anneau des nombres d'Eisenstein Z[ω].
    
    Puisque N ≡ 1 (mod 3), N se factorise comme:
    N = π · π̄  où π = a + bω est un nombre d'Eisenstein premier
    et π̄ = a + bω̄ est son conjugué.
    
    La norme: N(π) = a² - ab + b² = N
    
    Méthode: Algorithme de Cornacchia adapté
    1. Trouver x tel que x² ≡ -3 (mod N)
    2. Appliquer l'algorithme d'Euclide à (N, x)
    3. Trouver (a, b) tel que a² - ab + b² = N
    """
    print("=" * 70)
    print("  FACTORISATION DE N DANS Z[ω] (NOMBRES D'EISENSTEIN)")
    print("=" * 70)
    
    # ═══════════════════════════════════════════════════════════
    # ÉTAPE 1: Trouver √(-3) mod N via Cipolla
    # ═══════════════════════════════════════════════════════════
    print("\n[1] Recherche de √(-3) mod N via l'algorithme de Cipolla")
    print("-" * 50)
    
    # Cipolla: trouver a tel que a²+3 est un non-résidu quadratique mod N
    # puis calculer (a + √(a²+3))^((N+1)/2) mod N
    
    def cipolla_sqrt(n, p, max_tries=10000):
        """Compute √n mod p using Cipolla's algorithm."""
        # Check if n is a quadratic residue
        if pow(n, (p - 1) // 2, p) != 1:
            return None
        
        for a in range(2, max_tries):
            # Check if a² - n is a non-residue
            w2 = (a * a + 3) % p  # for sqrt(-3), we want a² + 3
            if pow(w2, (p - 1) // 2, p) == p - 1:
                # Found! Compute (a + √(a²+3))^((p+1)/2) in GF(p²)
                
                def gf2_mul(p1, p2, w2, mod):
                    a, b = p1
                    c, d = p2
                    return ((a*c + b*d*w2) % mod, (a*d + b*c) % mod)
                
                result = (1, 0)
                base = (a % p, 1)
                exp = (p + 1) // 2
                
                while exp > 0:
                    if exp & 1:
                        result = gf2_mul(result, base, w2, p)
                    base = gf2_mul(base, base, w2, p)
                    exp >>= 1
                
                return result[0]
        
        return None
    
    sqrt_neg3 = cipolla_sqrt(-3 % N, N)
    
    if sqrt_neg3:
        # Verify
        check = (sqrt_neg3 * sqrt_neg3) % N
        print(f"  √(-3) mod N trouvé!")
        print(f"  Vérification: (√(-3))² ≡ -3 (mod N): {check == (-3 + N) % N}")
    else:
        print(f"  ERREUR: √(-3) mod N non trouvé")
        return None
    
    # ═══════════════════════════════════════════════════════════
    # ÉTAPE 2: Algorithme de Cornacchia pour u² + 3v² = 4N
    # ═══════════════════════════════════════════════════════════
    print("\n[2] Algorithme de Cornacchia: u² + 3v² = 4N")
    print("-" * 50)
    
    # On cherche (u, v) tel que u² + 3v² = 4N
    # Puis a = (u+v)/2, b = v  (si u+v pair)
    # Et N = a² - ab + b² = N(π) où π = a + bω
    
    # Méthode: algorithme d'Euclide étendu sur (4N, u₀)
    # où u₀ = 2·√(-3) mod N (ou une autre racine pertinente)
    
    # En fait, la méthode standard pour factoriser p = a² - ab + b²:
    # 1. Trouver r = √(-3) mod p
    # 2. Appliquer l'algorithme d'Euclide: gcd(p, r)
    # 3. Le PREMIER reste < √p donne la solution
    
    # Essayons avec l'algorithme d'Euclide directement
    r0 = N
    r1 = sqrt_neg3
    
    print(f"  Algorithme d'Euclide sur (N, √(-3))...")
    
    sqrt_N = int(math.isqrt(N))
    step = 0
    
    while r1 > sqrt_N and step < 10000:
        r0, r1 = r1, r0 % r1
        step += 1
        if step % 500 == 0:
            print(f"  Étape {step}: r = {r1.bit_length()} bits")
    
    print(f"  Convergé en {step} étapes")
    print(f"  r_final = {r1}")
    print(f"  r_final bits = {r1.bit_length()}")
    print(f"  √N bits = {sqrt_N.bit_length()}")
    
    if r1 <= sqrt_N:
        # Vérifier: u² + 3v² = 4N avec u = r1
        u = r1
        val = 4 * N - u * u
        if val >= 0 and val % 3 == 0:
            v_sq = val // 3
            v = int(math.isqrt(v_sq))
            if v * v == v_sq:
                a = (u + v) // 2
                b = v
                norm = a*a - a*b + b*b
                print(f"\n  ★★★ FACTORISATION TROUVÉE! ★★★")
                print(f"  a = {a}")
                print(f"  b = {b}")
                print(f"  a bits: {a.bit_length()}, b bits: {b.bit_length()}")
                print(f"  Norme: a² - ab + b² = {norm}")
                print(f"  Norme == N: {norm == N}")
                print(f"  π = {a} + {b}·ω")
                print(f"  π̄ = {a} + {b}·ω̄")
                print(f"  N = π · π̄ dans Z[ω]")
                
                return a, b
            else:
                print(f"  v² = {v_sq}, isqrt = {v}, pas un carré parfait")
        else:
            print(f"  4N - u² = {val}, négatif ou pas divisible par 3")
    
    # Si la méthode directe ne marche pas, essayer avec l'autre racine
    print(f"\n  Essai avec l'autre racine: -√(-3) mod N...")
    
    sqrt_neg3_neg = N - sqrt_neg3
    r0 = N
    r1 = sqrt_neg3_neg
    
    step = 0
    while r1 > sqrt_N and step < 10000:
        r0, r1 = r1, r0 % r1
        step += 1
    
    print(f"  Convergé en {step} étapes")
    
    if r1 <= sqrt_N:
        u = r1
        val = 4 * N - u * u
        if val >= 0 and val % 3 == 0:
            v_sq = val // 3
            v = int(math.isqrt(v_sq))
            if v * v == v_sq:
                a = (u + v) // 2
                b = v
                norm = a*a - a*b + b*b
                print(f"\n  ★★★ FACTORISATION TROUVÉE! ★★★")
                print(f"  a = {a}")
                print(f"  b = {b}")
                print(f"  a bits: {a.bit_length()}, b bits: {b.bit_length()}")
                print(f"  Norme == N: {norm == N}")
                return a, b
    
    # Méthode alternative: chercher directement a² - ab + b² = N
    print(f"\n  Méthode alternative: recherche directe a² - ab + b² = N")
    print(f"  En balayant b de 1 à √N...")
    
    # b² ≤ N → b ≤ √N ≈ 2^128
    # Pour chaque b, résoudre a² - ab = N - b²
    # a(a-b) = N - b²
    # C'est un système quadratique: a² - ab + (b² - N) = 0
    # discriminant = b² - 4(b²-N) = 4N - 3b²
    # a = (b ± √(4N - 3b²)) / 2
    
    # Pour que a soit entier: 4N - 3b² doit être un carré parfait
    # et (b + √(4N-3b²)) doit être pair
    
    # Chercher b dans un range autour de N^(1/2) / √3
    # car le minimum de a² - ab + b² pour |a|,|b| fixés est ≈ (a²+b²)/2
    
    # b_max ≈ √(4N/3) ≈ 2^127.7
    b_max = int(math.isqrt(4 * N // 3))
    print(f"  b_max ≈ 2^{b_max.bit_length()}")
    print(f"  Recherche exhaustive impossible (2^128 valeurs)")
    
    # Mais on peut utiliser la méthode de Schoof ou la théorie 
    # des corps de nombres pour trouver la factorisation plus efficacement
    
    # Essayons une approche différente: utiliser la relation 
    # entre l'endomorphisme de la courbe et la factorisation
    
    # Sur secp256k1, on a déjà λ tel que λ² + λ + 1 ≡ 0 (mod N)
    # Dans Z[ω]: λ correspond à -ω (ou -ω²)
    # Car ω² + ω + 1 = 0 → -ω correspond à λ
    
    # L'idéal (N) dans Z[ω] se factorise comme:
    # (N) = (π)(π̄) où π est un élément de norme N
    
    # Pour trouver π, on peut utiliser:
    # π = a + bω où a, b sont déterminés par la factorisation
    # de N dans Z[ω]
    
    # Méthode de Cornacchia AMÉLIORÉE:
    # Au lieu de chercher u² + 3v² = 4N directement,
    # on peut utiliser la connaissance de λ pour guider la recherche
    
    print(f"\n  Méthode améliorée utilisant la connaissance de λ...")
    
    # λ ≡ -ω (mod N), donc λ² ≡ ω² (mod N)
    # On a: λ = N - ω dans Z[ω]/(N)
    # L'idéal (N, λ+ω) = (π) est un des facteurs
    
    # Pour trouver π, on peut calculer:
    # gcd(N, λ+ω) dans Z[ω]
    # Ce qui revient à trouver le PGCD dans Z[ω]
    
    # Le PGCD dans Z[ω] se calcule via l'algorithme d'Euclidean
    # dans Z[ω], qui est un anneau euclidien!
    
    # Division euclidienne dans Z[ω]:
    # Pour α, β ∈ Z[ω], ∃! q, r ∈ Z[ω]: α = qβ + r, N(r) < N(β)
    
    def eisenstein_gcd(alpha, beta):
        """
        Compute GCD in Z[ω] using the Euclidean algorithm.
        α = (a, b) represents a + bω
        Norm: N(a + bω) = a² - ab + b²
        """
        def norm(pair):
            a, b = pair
            return a*a - a*b + b*b
        
        def eisenstein_divide(alpha, beta):
            """
            Compute quotient and remainder of alpha/beta in Z[ω].
            Returns (q, r) such that alpha = q*beta + r, N(r) < N(beta).
            """
            a0, b0 = alpha
            a1, b1 = beta
            
            n_beta = norm(beta)
            if n_beta == 0:
                return (0, 0), alpha
            
            # Compute alpha * conjugate(beta) / N(beta)
            # conj(a + bω) = a + bω̄ = (a+b) - b·ω... 
            # Actually: ω̄ = ω² = -1-ω, so conj(a+bω) = a + b(-1-ω) = (a-b) + (-b)ω
            # Wait: conjugate in Z[ω]: conj(a + bω) = a + bω̄
            # ω̄ = ω² = -1-ω
            # conj(a + bω) = a + b(-1-ω) = (a-b) - bω
            
            conj_beta = (a1 - b1, -b1)
            
            # alpha * conj_beta in Z[ω]
            # (a0 + b0ω)(c0 + c1ω) = a0*c0 + a0*c1*ω + b0*ω*c0 + b0*c1*ω²
            # = a0*c0 + (a0*c1 + b0*c0)*ω + b0*c1*(-1-ω)
            # = (a0*c0 - b0*c1) + (a0*c1 + b0*c0 - b0*c1)*ω
            
            c0, c1 = conj_beta
            num_a = a0*c0 - b0*c1
            num_b = a0*c1 + b0*c0 - b0*c1
            
            # Divide by N(beta) and round
            q_a = round(num_a / n_beta)
            q_b = round(num_b / n_beta)
            
            # Compute remainder: r = alpha - q*beta
            # q*beta = (q_a + q_b*ω)(a1 + b1*ω)
            # = (q_a*a1 - q_b*b1) + (q_a*b1 + q_b*a1 - q_b*b1)*ω
            r_a = a0 - (q_a*a1 - q_b*b1)
            r_b = b0 - (q_a*b1 + q_b*a1 - q_b*b1)
            
            return (q_a, q_b), (r_a, r_b)
        
        # Euclidean algorithm
        while norm(beta) > 0:
            q, r = eisenstein_divide(alpha, beta)
            if norm(r) >= norm(beta):
                # Rounding error, try exact division
                break
            alpha = beta
            beta = r
        
        return alpha  # GCD
    
    # Compute gcd(N, λ + ω) in Z[ω]
    # N = (N, 0) in Z[ω]
    # λ + ω = (λ, 1) in Z[ω]  [representing λ + 1·ω]
    
    print(f"  Calcul de gcd(N, λ+ω) dans Z[ω]...")
    
    alpha = (N, 0)  # N + 0ω
    beta = (LAMBDA, 1)  # λ + ω
    
    gcd_result = eisenstein_gcd(alpha, beta)
    a_gcd, b_gcd = gcd_result
    norm_gcd = a_gcd**2 - a_gcd*b_gcd + b_gcd**2
    
    print(f"  gcd = ({a_gcd}) + ({b_gcd})·ω")
    print(f"  N(gcd) = {norm_gcd}")
    print(f"  N(gcd) == N: {norm_gcd == N}")
    print(f"  N(gcd) == 1: {norm_gcd == 1}")
    
    if norm_gcd == N:
        print(f"\n  ★★★ FACTORISATION DANS Z[ω] TROUVÉE VIA GCD! ★★★")
        print(f"  π = ({a_gcd}) + ({b_gcd})·ω")
        print(f"  π̄ = ({a_gcd - b_gcd}) + ({-b_gcd})·ω")
        print(f"  N = π · π̄ = {norm_gcd}")
        
        # Vérifier que π est bien un diviseur de N dans Z[ω]
        print(f"\n  Vérification:")
        
        # π̄
        a_conj = a_gcd - b_gcd
        b_conj = -b_gcd
        
        # π · π̄
        # (a + bω)(a' + b'ω) = (aa' - bb') + (ab' + ba' - bb')ω
        prod_a = a_gcd * a_conj - b_gcd * b_conj
        prod_b = a_gcd * b_conj + b_gcd * a_conj - b_gcd * b_conj
        
        # Dans Z[ω]/(N), le produit devrait être N + 0ω
        prod_a_mod = prod_a % N
        prod_b_mod = prod_b % N
        
        print(f"  π · π̄ = ({prod_a}) + ({prod_b})·ω")
        print(f"  π · π̄ mod N = ({prod_a_mod}) + ({prod_b_mod})·ω")
        print(f"  Partie réelle mod N == 0: {prod_a_mod == 0}")
        print(f"  Partie ω mod N == 0: {prod_b_mod == 0}")
        
        if prod_a_mod == 0 and prod_b_mod == 0:
            print(f"\n  ★ VÉRIFICATION RÉUSSIE: π · π̄ ≡ 0 dans Z[ω]/(N)")
            print(f"  π est bien un diviseur de N dans Z[ω]!")
        
        return a_gcd, b_gcd
    
    elif norm_gcd == 1:
        print(f"  Le GCD est une unité → N est premier dans Z[ω]?")
        print(f"  Mais N ≡ 1 (mod 3), donc N devrait être scindé...")
        print(f"  Problème possible avec la division euclidienne")
    else:
        print(f"  N(gcd) = {norm_gcd} (ni N ni 1)")
        print(f"  Facteur potentiel de N: {norm_gcd}")
        print(f"  N / N(gcd) = {N // norm_gcd}")
        print(f"  N % N(gcd) = {N % norm_gcd}")
        
        if N % norm_gcd == 0:
            print(f"\n  ★ N(gcd) DIVISE N! C'est un facteur propre!")
            print(f"  Mais N est premier dans Z, donc c'est impossible...")
            print(f"  Sauf si N(gcd) = N (facteur trivial) ou N(gcd) = 1")
    
    return None


def explore_hybrid_attack():
    """
    Explorer l'attaque hybride lattice + algébrique.
    
    L'idée: combiner la structure du réseau idéal dans Z[ω]
    avec la linéarité partielle de SHA-256 au round 0.
    """
    print("\n" + "=" * 70)
    print("  ATTAQUE HYBRIDE LATTICE + ALGÉBRIQUE")
    print("=" * 70)
    
    print("""
  CONCEPT:
  ═══════
  
  L'attaque combine 3 éléments:
  
  1. DÉCOMPOSITION GLV: d = d₀ + d₁·λ mod N
     Q = d₀·G + d₁·(λ·G)
     Les points G et λ·G sont publics.
  
  2. STRUCTURE DU RÉSEAU IDÉAL:
     L'idéal (π) dans Z[ω] encode la relation entre d₀ et d₁.
     Les vecteurs courts du réseau idéal donnent les décompositions
     optimales de d.
  
  3. LINÉARITÉ AU ROUND 0:
     Le round 0 de SHA-256 est PARTIELLEMENT linéaire.
     Les bits de x (coordonnée EC) influencent le round 0
     de manière prévisible (pour les premiers bits).
  
  SCHÉMA D'ATTAQUE:
  ════════════════
  
  Étape 1: Décomposer Q = d₀·G + d₁·H (où H = λ·G)
  Étape 2: Pour chaque candidat (d₀, d₁), calculer le point
           P = d₀·G + d₁·H et obtenir x = P.x
  Étape 3: Vérifier si le round 0 de SHA-256(02||x) ou
           SHA-256(03||x) est compatible avec la cible
  Étape 4: Si compatible, vérifier l'adresse complète
  
  L'étape 3 est la CLEF: au lieu de vérifier l'adresse complète
  (coûteux: SHA-256 + RIPEMD-160 + Base58), on ne vérifie que
  le round 0 de SHA-256, qui est BEAUCOUP plus rapide.
  
  Le round 0 ne fournit qu'un FILTRE PARTIEL: il élimine ~50%
  des candidats (1 bit d'information). Mais chaque élimination
  économise le coût de la vérification complète.
  
  PROBLÈME: Le filtre du round 0 est trop faible.
  Avec 2^67 candidats, on élimine seulement la moitié → 2^66 restent.
  
  AMÉLIORATION: Utiliser PLUSIEURS rounds comme filtre.
  Rounds 0-3: degré ≈ 18, mais on peut évaluer RAPIDEMENT
  car on n'a besoin que des 8 mots d'état (256 bits).
  
  Chaque round fournit ~128 bits d'information (après diffusion).
  Après 3 rounds: ~128 bits de filtre.
  Mais la relation entre d et l'état du round 3 est non-linéaire...
  
  Le filtre ne fonctionne QUE si on peut prédire l'état des rounds
  à partir de d. Ce qui nécessite de calculer d·G, puis SHA-256.
  Ce qui est aussi coûteux que la vérification complète!
  
  CONCLUSION:
  ══════════
  
  L'attaque hybride ne fournit PAS d'accélération par rapport à
  la recherche exhaustive. Le goulot d'étranglement est le calcul
  de d·G pour chaque candidat, qui est O(log d) multiplications EC.
  
  La seule façon d'accélérer est de RÉDUIRE le nombre de candidats
  via le réseau idéal. Si on pouvait réduire de 2^134 à 2^67
  (via GLV + MITM), ce serait une accélération de 2^67.
  
  Mais le MITM nécessite 2^67 stockage, ce qui est hors de portée.
  
  LA VRAIE QUESTION: Existe-t-il un algorithme qui exploite
  la structure du réseau idéal pour faire mieux que le MITM standard?
  
  Réponse: PAS DANS LA LITTÉRATURE ACTUELLE.
  Mais c'est là que la recherche devrait se concentrer.
    """)
    
    # ═══════════════════════════════════════════════════════════
    # Test pratique: filtre round 0
    # ═══════════════════════════════════════════════════════════
    print("\n  Test pratique: efficacité du filtre round 0")
    print("-" * 50)
    
    import struct
    
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
    
    TARGET_PUBKEY = "02145d2611c823a396ef6712ce0f712f09b9b4f3135e3e0aa3230fb9b6d08d1e16"
    
    # Compute target round 0 state
    def sha256_round0_only(data):
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
            new_a = (t1 + t2) & 0xFFFFFFFF
            new_e = (d + t1) & 0xFFFFFFFF
            return (new_a, b, c, d, new_e, f, g, hh)
    
    target_r0 = sha256_round0_only(bytes.fromhex(TARGET_PUBKEY))
    
    # Test: combien de clés aléatoires passent le filtre round 0?
    # Filtre: vérifier si certains bits du round 0 matchent
    
    # On utilise les 8 bits de poids faible de chaque mot comme filtre
    # Cela donne 8*8 = 64 bits de filtre
    
    # Créer le filtre à partir du target
    filter_mask = 0xFF  # 8 bits de poids faible
    target_filter = tuple(w & filter_mask for w in target_r0)
    
    print(f"  Filtre cible (8 LSB par mot): {target_filter}")
    
    # Tester avec des clés aléatoires
    n_test = 10000
    passes_filter = 0
    
    start = time.time()
    for i in range(n_test):
        d = 2**134 + random.randint(0, 2**134 - 1)
        Q = GENERATOR * d
        x, y = Q.x(), Q.y()
        prefix = '02' if y % 2 == 0 else '03'
        comp = prefix + f'{x:064x}'
        
        r0 = sha256_round0_only(bytes.fromhex(comp))
        candidate_filter = tuple(w & filter_mask for w in r0)
        
        if candidate_filter == target_filter:
            passes_filter += 1
    
    elapsed = time.time() - start
    filter_rate = passes_filter / n_test
    
    print(f"  Test: {n_test} clés aléatoires")
    print(f"  Passent le filtre: {passes_filter} ({filter_rate*100:.3f}%)")
    print(f"  Taux attendu: {1/2**64*100:.10f}% (si 64 bits indépendants)")
    print(f"  Vitesse: {n_test/elapsed:.0f} clés/s")
    
    # Le filtre est BEAUCOUP plus permissif que prévu car les 8 LSB
    # de chaque mot ne sont PAS indépendants — ils sont corrélés
    # par la structure du round 0
    
    if filter_rate > 0.01:
        print(f"\n  Le filtre est trop permissif ({filter_rate*100:.1f}% passent)")
        print(f"  Les bits du round 0 sont fortement corrélés")
        print(f"  On ne peut PAS utiliser le round 0 comme filtre efficace")
    else:
        print(f"\n  Le filtre élimine {(1-filter_rate)*100:.1f}% des candidats!")
        print(f"  C'est une accélération de {1/filter_rate:.0f}x")


# ============================================================================
# MAIN
# ============================================================================
if __name__ == "__main__":
    print("◆" * 35)
    print("  VORTEX PRIME — Factorisation Z[ω] + Attaque Hybride")
    print("◆" * 35 + "\n")
    
    total_start = time.time()
    
    # Factorisation dans Z[ω]
    result = factor_N_in_eisenstein()
    
    if result:
        a, b = result
        print(f"\n\n  ★ RÉSULTAT: N = ({a}) + ({b})·ω  dans Z[ω]")
        print(f"  Facteur: π = {a} + {b}·ω")
        print(f"  Conjugué: π̄ = {a-b} + {-b}·ω")
        print(f"  Norme: {a**2 - a*b + b**2}")
    else:
        print(f"\n  Factorisation dans Z[ω] non trouvée par les méthodes testées")
        print(f"  L'algorithme de division euclidienne dans Z[ω] nécessite")
        print(f"  une implémentation plus robuste pour les grands nombres")
    
    # Attaque hybride
    explore_hybrid_attack()
    
    total = time.time() - total_start
    print(f"\nTemps total: {total:.1f}s")
