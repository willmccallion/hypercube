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

# Patterns where bit-fixing hits Theta(sqrt(N)) worst case.
ADVERSARIAL = {"bit-reversal", "transpose"}
# Patterns where bit-fixing is trivially good (congestion ~1).
EASY_FOR_GREEDY = {"complement", "butterfly"}

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
    # Average over runs per (dimension, strategy, traffic) group.
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
    """Filter to only rows where all packets were delivered (delivery_rate >= 99.9%).

    This removes tick-budget artifacts (e.g., shuffle at dim=16) that would
    distort congestion and latency charts with meaningless queue-overflow numbers.
    """
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

        # Theoretical sqrt(N) reference line for adversarial patterns.
        if traffic in ADVERSARIAL:
            dims = sorted(subset["dimension"].unique())
            sqrt_n = [2 ** (d / 2) for d in dims]
            ax.plot(
                dims,
                sqrt_n,
                color="gray",
                linestyle=":",
                alpha=0.6,
                label=r"$\sqrt{N}$ (theory)",
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
        ax.legend(fontsize=9)

    # Hide unused subplots.
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
        ax.legend(fontsize=9)

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
    """Average latency vs. average hops — shows pipelining and queuing effects."""
    df = fully_delivered(df)
    fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(14, 6))

    # Left: bit-fixing. Right: Valiant.
    for strategy, ax, title in [
        ("bit-fixing", ax1, "Bit-Fixing"),
        ("valiant", ax2, "Valiant"),
    ]:
        subset = df[df["strategy"] == strategy]
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
            # Connect points with a thin line to show progression.
            ax.plot(t["avg_hops"], t["avg_latency"], alpha=0.3, linewidth=1)

        ax.set_xlim(left=-0.5)
        ax.set_ylim(bottom=-0.5)
        ax.set_xlabel("avg hops")
        ax.set_ylabel("avg latency (ticks)")
        ax.set_title(title, fontweight="bold")
        ax.legend(fontsize=8)

    fig.suptitle(
        "Latency vs. Hops\n"
        "(synchronous model: packets traverse ~2 hops/tick via pipelining)",
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

        # Theoretical reference: n/2 average hops for random routing.
        dims = sorted(subset["dimension"].unique())
        ax.plot(
            dims,
            [d / 2 for d in dims],
            color="gray",
            linestyle=":",
            alpha=0.6,
            label="n/2 (expected random)",
        )

        ax.set_title(title, fontweight="bold")
        ax.set_xlabel("dimension (n)")
        ax.set_ylabel("avg hops")
        ax.legend(fontsize=9)

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
