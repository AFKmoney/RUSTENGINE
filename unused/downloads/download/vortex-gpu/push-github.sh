#!/bin/bash
# VORTEX PRIME v4 — GitHub Push Script
# ================================
# To push to GitHub, you need a GitHub account and personal access token.
#
# Usage:
#   1. Create a GitHub repo: https://github.com/new
#      Name: vortex-prime (or any name)
#      Make it PRIVATE (this is cryptanalysis code!)
#
#   2. Create a Personal Access Token:
#      https://github.com/settings/tokens
#      Scopes: repo, workflow
#
#   3. Run this script:
#      export GH_TOKEN=ghp_your_token_here
#      ./push-github.sh
#
# OR manually:
#   git remote add origin https://github.com/YOUR_USERNAME/vortex-prime.git
#   git push -u origin main

set -e

REPO_NAME="vortex-prime"
REMOTE_URL="${1:-}"

if [ -z "$GH_TOKEN" ] && [ -z "$REMOTE_URL" ]; then
    echo "ERROR: No GitHub token or remote URL provided."
    echo ""
    echo "Option 1: Set GH_TOKEN environment variable"
    echo "  export GH_TOKEN=ghp_your_token_here"
    echo "  ./push-github.sh"
    echo ""
    echo "Option 2: Provide remote URL directly"
    echo "  ./push-github.sh https://github.com/user/vortex-prime.git"
    echo ""
    echo "The code is committed locally and ready to push."
    echo "Run 'git log --oneline' to see commits."
    exit 1
fi

cd /home/z/my-project

if [ -n "$GH_TOKEN" ]; then
    # Use gh CLI to create repo and push
    export PATH="/home/z/.local/bin:$PATH"

    echo "Authenticating with GitHub..."
    echo "$GH_TOKEN" | gh auth login --with-token

    echo "Creating repository: $REPO_NAME"
    gh repo create "$REPO_NAME" --private --source=. --push --description "VORTEX PRIME v4 — GPU-Accelerated Cryptanalytic Solver for Bitcoin Puzzle #135"

    echo "✓ Pushed to GitHub!"
    gh repo view --web
elif [ -n "$REMOTE_URL" ]; then
    echo "Adding remote: $REMOTE_URL"
    git remote add origin "$REMOTE_URL" 2>/dev/null || git remote set-url origin "$REMOTE_URL"

    echo "Pushing to GitHub..."
    git push -u origin main

    echo "✓ Pushed to GitHub!"
fi
