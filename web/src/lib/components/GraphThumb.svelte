<script lang="ts">
	import type { RgxfJson } from '$lib/api';
	import { decodeRgxf } from '$lib/rgxf';

	let { rgxf, size = 48 }: { rgxf: RgxfJson; size?: number } = $props();

	let canvas = $state<HTMLCanvasElement | null>(null);

	const matrix = $derived(decodeRgxf(rgxf));

	$effect(() => {
		if (!canvas) return;
		const n = rgxf.n;
		const adj = matrix;
		const dpr = window.devicePixelRatio || 1;

		canvas.width = size * dpr;
		canvas.height = size * dpr;
		const ctx = canvas.getContext('2d')!;
		ctx.scale(dpr, dpr);

		// Fill background matching MatrixView
		ctx.fillStyle = '#141420';
		ctx.fillRect(0, 0, size, size);

		if (n === 0) return;

		// Rotated 45°: cell (i,j) → pixel at (j-i+(n-1), i+j)
		// Grid spans (2n-1) × (2n-1) in rotated coords
		const gridW = 2 * n - 1;
		const cellSize = size / gridW;

		// Colors matching MatrixView / CircleLayout palette
		const EDGE = [99, 102, 241];     // #6366f1 (indigo)
		const NON_EDGE = [10, 10, 15];   // #0a0a0f
		const DIAG = [26, 26, 46];       // #1a1a2e

		// ── Render sharp diamond into an offscreen canvas ──
		const offscreen = document.createElement('canvas');
		offscreen.width = canvas.width;
		offscreen.height = canvas.height;
		const off = offscreen.getContext('2d')!;
		off.scale(dpr, dpr);
		off.fillStyle = '#141420';
		off.fillRect(0, 0, size, size);

		for (let i = 0; i < n; i++) {
			for (let j = 0; j < n; j++) {
				const rx = j - i + (n - 1);
				const ry = i + j;
				const px = rx * cellSize;
				const py = ry * cellSize;
				const cs = cellSize + 0.5; // slight overlap to avoid gaps

				if (i === j) {
					// Spine — brighter indigo accent
					off.fillStyle = `rgb(${DIAG[0]}, ${DIAG[1]}, ${DIAG[2]})`;
					off.fillRect(px, py, cs, cs);
				} else if (adj[i][j]) {
					// Edge — indigo, slightly modulated by position for texture
					const t = (i + j) / (2 * (n - 1)); // 0–1 position along diamond
					const r = EDGE[0] + Math.round(t * 20 - 10);
					const g = EDGE[1] + Math.round(t * 20 - 10);
					const b = EDGE[2];
					off.fillStyle = `rgb(${r}, ${g}, ${b})`;
					off.fillRect(px, py, cs, cs);
				} else {
					// Non-edge — near-black
					off.fillStyle = `rgb(${NON_EDGE[0]}, ${NON_EDGE[1]}, ${NON_EDGE[2]})`;
					off.fillRect(px, py, cs, cs);
				}
			}
		}

		// ── Soften with a blur pass then composite ──
		// Draw blurred version as base, then layer sharp version at partial opacity
		const blurPx = Math.max(0.8, size / 60);
		ctx.filter = `blur(${blurPx}px)`;
		ctx.drawImage(offscreen, 0, 0, size, size);
		ctx.filter = 'none';

		// Layer the sharp version on top at reduced opacity for detail
		ctx.globalAlpha = 0.55;
		ctx.drawImage(offscreen, 0, 0, size, size);
		ctx.globalAlpha = 1.0;
	});
</script>

<canvas
	bind:this={canvas}
	style="width: {size}px; height: {size}px"
	class="creature-thumb"
></canvas>

<style>
	.creature-thumb {
		border-radius: 0.375rem;
		border: 1px solid var(--color-border);
		display: block;
	}
</style>
