<script lang="ts">
	import type { RgxfJson } from '$lib/api';
	import { decodeRgxf } from '$lib/rgxf';

	let { rgxf, witness = [], size = 400 }: { rgxf: RgxfJson; witness?: number[]; size?: number } =
		$props();

	let canvas: HTMLCanvasElement;

	const witnessSet = $derived(new Set(witness));
	const matrix = $derived(decodeRgxf(rgxf));

	$effect(() => {
		if (!canvas) return;
		const n = rgxf.n;
		const ctx = canvas.getContext('2d');
		if (!ctx) return;

		const dpr = window.devicePixelRatio || 1;
		canvas.width = size * dpr;
		canvas.height = size * dpr;
		canvas.style.width = `${size}px`;
		canvas.style.height = `${size}px`;
		ctx.scale(dpr, dpr);

		ctx.fillStyle = '#141420';
		ctx.fillRect(0, 0, size, size);

		if (n === 0) return;

		// ── Layout: rotated 45° diamond ──
		// Cell (i,j) maps to rotated coords (j-i+(n-1), i+j)
		// Grid is (2n-1) × (2n-1) in rotated space
		const gridW = 2 * n - 1;

		const cellSize = size / gridW;
		const ox = 0;
		const oy = 0;

		// ── Draw cells ──
		for (let i = 0; i < n; i++) {
			for (let j = 0; j < n; j++) {
				const rx = j - i + (n - 1);
				const ry = i + j;
				const px = ox + rx * cellSize;
				const py = oy + ry * cellSize;

				if (i === j) {
					ctx.fillStyle = '#1a1a2e';
					ctx.fillRect(px, py, cellSize + 0.5, cellSize + 0.5);
					continue;
				}

				const isWitnessCell = witnessSet.has(i) && witnessSet.has(j);

				if (matrix[i][j]) {
					ctx.fillStyle = isWitnessCell ? '#ef4444' : '#6366f1';
				} else {
					ctx.fillStyle = isWitnessCell ? '#3a1111' : '#0a0a0f';
				}
				ctx.fillRect(px, py, cellSize + 0.5, cellSize + 0.5);
			}
		}

		// ── Grid lines along diamond edges ──
		ctx.strokeStyle = '#2a2a3a';
		ctx.lineWidth = 0.5;

		// Lines parallel to top-left edge (constant i)
		for (let i = 0; i <= n; i++) {
			// Row i starts at rotated (0-i+(n-1), i+0) = (n-1-i, i)
			// Row i ends at rotated ((n-1)-i+(n-1), i+(n-1)) = (2n-2-i, i+n-1)
			// But for grid lines between rows, use i boundary:
			// top edge of row i: from cell (i,0) to cell (i,n-1)
			const x0 = ox + (0 - i + (n - 1)) * cellSize;
			const y0 = oy + (i + 0) * cellSize;
			const x1 = ox + ((n - 1) - i + (n - 1)) * cellSize + cellSize;
			const y1 = oy + (i + (n - 1)) * cellSize + cellSize;
			// This traces a diagonal — need the top-left edge of each cell in row i
			// Actually, draw lines along the two diamond axes
		}

		// Simpler approach: draw subtle grid lines for each row and column of the rotated grid
		// Horizontal "rows" in rotated space: constant (i+j) lines
		for (let s = 0; s < 2 * n; s++) {
			// Sum i+j = s. Valid cells: max(0,s-n+1) <= i <= min(s,n-1)
			const iMin = Math.max(0, s - n + 1);
			const iMax = Math.min(s, n - 1);
			if (iMin > iMax) continue;
			const jMin = s - iMax;
			const jMax = s - iMin;
			const x0 = ox + (jMin - iMax + (n - 1)) * cellSize;
			const x1 = ox + (jMax - iMin + (n - 1)) * cellSize + cellSize;
			const y = oy + s * cellSize;
			ctx.beginPath();
			ctx.moveTo(x0, y);
			ctx.lineTo(x1, y);
			ctx.stroke();
		}

		// Vertical "columns" in rotated space: constant (j-i) lines
		for (let d = -(n - 1); d <= n; d++) {
			// Diff j-i = d. Valid: max(0,-d) <= i <= min(n-1,n-1-d)
			const iMin = Math.max(0, -d);
			const iMax = Math.min(n - 1, n - 1 - d);
			if (iMin > iMax) continue;
			const sMin = iMin + (iMin + d); // i+j where j=i+d
			const sMax = iMax + (iMax + d);
			const x = ox + (d + (n - 1)) * cellSize;
			const y0 = oy + sMin * cellSize;
			const y1 = oy + sMax * cellSize + cellSize;
			ctx.beginPath();
			ctx.moveTo(x, y0);
			ctx.lineTo(x, y1);
			ctx.stroke();
		}

	});
</script>

<canvas bind:this={canvas} class="matrix-canvas" aria-label="Adjacency matrix visualization"></canvas>

<style>
	.matrix-canvas {
		border-radius: 0.5rem;
		border: 1px solid var(--color-border);
	}
</style>
