#!/usr/bin/env python3
"""Generate publication-quality charts from hypercube sweep CSV data.

Usage:
    .venv/bin/python scripts/plot.py full_study.csv
    .venv/bin/python scripts/plot.py full_study.csv --outdir charts/
"""

import argparse
import math
from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np
import pandas as pd

STRATEGY_COLORS = {"bit-fixing": "#d62728", "valiant": "#1f77b4"}
STRATEGY_STYLES = {"bit-fixing": "--", "valiant": "-"}
TRAFFIC_MARKERS = {
    "bit-reversal": "o",
    "complement": "s",
    "transpose": "D",
    "butterfly": "^",
    "shuffle": "v",
    "random": "x",
}

ADVERSARIAL = {"bit-reversal", "transpose"}
EASY_FOR_GREEDY = {"complement", "butterfly"}

# Shared reference line style: thin dotted black.
REF_STYLE = dict(color="black", linestyle=":", linewidth=1.5, alpha=0.6)
# Light green fill for "expected" theory regions.
BAND_COLOR = "#b6d7a8"
BAND_ALPHA = 0.25

plt.rcParams.update(
    {
        "figure.facecolor": "white",
        "axes.facecolor": "white",
        "axes.grid": True,
        "grid.alpha": 0.3,
        "font.size": 11,
        "figure.dpi": 150,
    }
)


def load(path: str) -> pd.DataFrame:
    df = pd.read_csv(path)
    grouped = (
        df.groupby(["dimension", "strategy", "traffic"])
        .agg(
            nodes=("nodes", "first"),
            max_congestion=("max_congestion", "mean"),
            avg_congestion=("avg_congestion", "mean"),
            max_latency=("max_latency", "mean"),
            avg_latency=("avg_latency", "mean"),
            max_hops=("max_hops", "mean"),
            avg_hops=("avg_hops", "mean"),
            delivered=("delivered", "mean"),
            total_packets=("total_packets", "first"),
        )
        .reset_index()
    )
    grouped["delivery_rate"] = grouped["delivered"] / grouped["total_packets"]
    return grouped


def fully_delivered(df: pd.DataFrame) -> pd.DataFrame:
    """Filter to only rows where all packets were delivered (delivery_rate >= 99.9%)."""
    return df[df["delivery_rate"] >= 0.999].copy()


def plot_congestion_scaling(df: pd.DataFrame, outdir: Path):
    """Max congestion vs. dimension for each traffic pattern, comparing strategies."""
    df = fully_delivered(df)
    traffic_patterns = sorted(df["traffic"].unique())
    ncols = min(3, len(traffic_patterns))
    nrows = math.ceil(len(traffic_patterns) / ncols)

    fig, axes = plt.subplots(nrows, ncols, figsize=(5.5 * ncols, 4.5 * nrows))
    if nrows * ncols == 1:
        axes = [axes]
    else:
        axes = axes.flatten()

    for idx, traffic in enumerate(traffic_patterns):
        ax = axes[idx]
        subset = df[df["traffic"] == traffic]
        dims = np.array(sorted(subset["dimension"].unique()))

        # Green shaded band for expected bit-fixing congestion.
        if traffic in ADVERSARIAL:
            exact = np.array([2 ** ((d - 2) // 2) for d in dims])
            # Band: 0.7x to 1.3x of theoretical (tight band around prediction).
            ax.fill_between(dims, exact * 0.7, exact * 1.3,
                            color=BAND_COLOR, alpha=BAND_ALPHA, label="expected (theory)")
            ax.plot(dims, exact, **REF_STYLE, label=r"$2^{\lfloor(n{-}2)/2\rfloor}$")
        elif traffic == "shuffle":
            ax.axhspan(1.5, 2.5, color=BAND_COLOR, alpha=BAND_ALPHA, label="expected (theory)")
            ax.axhline(2, **REF_STYLE, label="congestion = 2")
        elif traffic in EASY_FOR_GREEDY:
            ax.axhspan(0.7, 1.3, color=BAND_COLOR, alpha=BAND_ALPHA, label="expected (theory)")
            ax.axhline(1, **REF_STYLE, label="congestion = 1")

        for strategy in ["bit-fixing", "valiant"]:
            s = subset[subset["strategy"] == strategy].sort_values("dimension")
            if s.empty:
                continue
            ax.plot(
                s["dimension"],
                s["max_congestion"],
                color=STRATEGY_COLORS[strategy],
                linestyle=STRATEGY_STYLES[strategy],
                marker="o",
                markersize=5,
                label=strategy,
                linewidth=2,
            )

        subtitle = ""
        if traffic in ADVERSARIAL:
            subtitle = " (adversarial for greedy)"
        elif traffic in EASY_FOR_GREEDY:
            subtitle = " (trivial for greedy)"
        ax.set_title(f"{traffic}{subtitle}", fontweight="bold", fontsize=10)
        ax.set_xlabel("dimension (n)")
        ax.set_ylabel("max congestion")
        ax.set_yscale("log", base=2)
        ax.legend(fontsize=8)

    for idx in range(len(traffic_patterns), len(axes)):
        axes[idx].set_visible(False)

    fig.suptitle("Max Congestion vs. Dimension by Traffic Pattern", fontweight="bold", y=1.01)
    fig.tight_layout()
    fig.savefig(outdir / "congestion_scaling.png", bbox_inches="tight")
    plt.close(fig)


def plot_strategy_comparison(df: pd.DataFrame, outdir: Path):
    """Side-by-side: bit-fixing vs. Valiant congestion across all traffic patterns."""
    df = fully_delivered(df)
    fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(13, 5.5), sharey=True)

    for strategy, ax, title in [
        ("bit-fixing", ax1, "Bit-Fixing (Greedy)"),
        ("valiant", ax2, "Valiant (Randomized)"),
    ]:
        subset = df[df["strategy"] == strategy]
        dims = np.array(sorted(subset["dimension"].unique()))

        # Theory shading.
        if strategy == "bit-fixing":
            exact = np.array([2 ** ((d - 2) // 2) for d in dims])
            ax.fill_between(dims, exact * 0.7, exact * 1.3,
                            color=BAND_COLOR, alpha=BAND_ALPHA,
                            label=r"$2^{\lfloor(n{-}2)/2\rfloor}$ band")
            ax.plot(dims, exact, **REF_STYLE, label=r"$2^{\lfloor(n{-}2)/2\rfloor}$")
        else:
            # Valiant congestion tracks ~n/2 empirically.
            half = dims / 2
            ax.fill_between(dims, half, half + 2,
                            color=BAND_COLOR, alpha=BAND_ALPHA,
                            label="n/2 band")
            ax.plot(dims, half, **REF_STYLE, label="n/2")

        for traffic in sorted(subset["traffic"].unique()):
            t = subset[subset["traffic"] == traffic].sort_values("dimension")
            ax.plot(
                t["dimension"],
                t["max_congestion"],
                marker=TRAFFIC_MARKERS.get(traffic, "o"),
                markersize=5,
                label=traffic,
                linewidth=1.8,
            )

        ax.set_title(title, fontweight="bold")
        ax.set_xlabel("dimension (n)")
        ax.set_ylabel("max congestion")
        ax.set_yscale("log", base=2)
        ax.legend(fontsize=8)

    fig.suptitle(
        "Congestion Scaling: Greedy vs. Randomized Routing\n"
        "(only configurations with 100% delivery)",
        fontweight="bold",
        y=1.02,
    )
    fig.tight_layout()
    fig.savefig(outdir / "strategy_comparison.png", bbox_inches="tight")
    plt.close(fig)


def plot_latency_vs_hops(df: pd.DataFrame, outdir: Path):
    """Average latency vs. average hops; the gap above latency = hops is queueing."""
    df = fully_delivered(df)
    fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(14, 6))

    for strategy, ax, title in [
        ("bit-fixing", ax1, "Bit-Fixing"),
        ("valiant", ax2, "Valiant"),
    ]:
        subset = df[df["strategy"] == strategy]
        max_h = subset["avg_hops"].max() * 1.1

        h = np.linspace(0, max_h, 100)
        ax.plot(h, h, **REF_STYLE, label="latency = hops (no queueing)")

        for traffic in sorted(subset["traffic"].unique()):
            t = subset[subset["traffic"] == traffic].sort_values("dimension")
            ax.scatter(
                t["avg_hops"],
                t["avg_latency"],
                marker=TRAFFIC_MARKERS.get(traffic, "o"),
                s=40,
                alpha=0.8,
                label=traffic,
            )
            ax.plot(t["avg_hops"], t["avg_latency"], alpha=0.3, linewidth=1)

        ax.set_xlim(left=-0.5)
        ax.set_ylim(bottom=-0.5)
        ax.set_xlabel("avg hops")
        ax.set_ylabel("avg latency (ticks)")
        ax.set_title(title, fontweight="bold")
        ax.legend(fontsize=8)

    fig.suptitle(
        "Latency vs. Hops\n"
        "(each link sends one packet per tick; time above the line is queueing)",
        fontweight="bold",
        y=1.02,
    )
    fig.tight_layout()
    fig.savefig(outdir / "latency_vs_hops.png", bbox_inches="tight")
    plt.close(fig)


def plot_hop_distribution(df: pd.DataFrame, outdir: Path):
    """Average hop count vs. dimension for each strategy and traffic pattern."""
    df = fully_delivered(df)
    fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(13, 5.5), sharey=True)

    for strategy, ax, title in [
        ("bit-fixing", ax1, "Bit-Fixing (Greedy)"),
        ("valiant", ax2, "Valiant (Randomized)"),
    ]:
        subset = df[df["strategy"] == strategy]
        dims = np.array(sorted(subset["dimension"].unique()))

        if strategy == "bit-fixing":
            # Most patterns cluster between 1 and n. Shade n/2 band.
            ax.fill_between(dims, dims * 0.35, dims * 0.65,
                            color=BAND_COLOR, alpha=BAND_ALPHA,
                            label="n/2 band")
            ax.plot(dims, dims / 2, **REF_STYLE, label="n/2")
            ax.plot(dims, dims, color="black", linestyle="--",
                    linewidth=1.2, alpha=0.35, label="n (all dims)")
        else:
            # Valiant: all patterns converge to ~n hops.
            ax.fill_between(dims, dims * 0.85, dims * 1.05,
                            color=BAND_COLOR, alpha=BAND_ALPHA,
                            label="~n band")
            ax.plot(dims, dims, **REF_STYLE, label="n (two n/2 phases)")

        for traffic in sorted(subset["traffic"].unique()):
            t = subset[subset["traffic"] == traffic].sort_values("dimension")
            ax.plot(
                t["dimension"],
                t["avg_hops"],
                marker=TRAFFIC_MARKERS.get(traffic, "o"),
                markersize=5,
                label=traffic,
                linewidth=1.8,
            )

        ax.set_title(title, fontweight="bold")
        ax.set_xlabel("dimension (n)")
        ax.set_ylabel("avg hops")
        ax.legend(fontsize=8)

    fig.suptitle(
        "Average Hop Count vs. Dimension\n"
        "(Valiant doubles path length by routing through a random intermediate)",
        fontweight="bold",
        y=1.02,
    )
    fig.tight_layout()
    fig.savefig(outdir / "hop_distribution.png", bbox_inches="tight")
    plt.close(fig)


def main():
    parser = argparse.ArgumentParser(description="Plot hypercube sweep results.")
    parser.add_argument("csv", help="Path to sweep CSV file")
    parser.add_argument(
        "--outdir",
        default="charts",
        help="Output directory for PNG charts (default: charts/)",
    )
    args = parser.parse_args()

    outdir = Path(args.outdir)
    outdir.mkdir(parents=True, exist_ok=True)

    df = load(args.csv)

    plot_congestion_scaling(df, outdir)
    plot_strategy_comparison(df, outdir)
    plot_latency_vs_hops(df, outdir)
    plot_hop_distribution(df, outdir)

    print(f"Charts written to {outdir}/")
    for f in sorted(outdir.glob("*.png")):
        print(f"  {f}")


if __name__ == "__main__":
    main()
