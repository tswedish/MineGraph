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

		const labelSpace = n > 20 ? 16 : 24;
		const cellArea = size - labelSpace;
		const cellSize = cellArea / n;

		// Draw cells
		for (let i = 0; i < n; i++) {
			for (let j = 0; j < n; j++) {
				const x = labelSpace + j * cellSize;
				const y = labelSpace + i * cellSize;

				if (i === j) {
					ctx.fillStyle = '#1a1a2e';
					ctx.fillRect(x, y, cellSize, cellSize);
					continue;
				}

				const isWitnessCell = witnessSet.has(i) && witnessSet.has(j);

				if (matrix[i][j]) {
					ctx.fillStyle = isWitnessCell ? '#ef4444' : '#6366f1';
				} else {
					ctx.fillStyle = isWitnessCell ? '#3a1111' : '#0a0a0f';
				}
				ctx.fillRect(x, y, cellSize, cellSize);
			}
		}

		// Grid lines
		ctx.strokeStyle = '#2a2a3a';
		ctx.lineWidth = 0.5;
		for (let i = 0; i <= n; i++) {
			const pos = labelSpace + i * cellSize;
			ctx.beginPath();
			ctx.moveTo(labelSpace, pos);
			ctx.lineTo(size, pos);
			ctx.stroke();
			ctx.beginPath();
			ctx.moveTo(pos, labelSpace);
			ctx.lineTo(pos, size);
			ctx.stroke();
		}

		// Vertex labels
		const fontSize = Math.max(8, Math.min(12, cellSize * 0.6));
		ctx.font = `${fontSize}px monospace`;
		ctx.textAlign = 'center';
		ctx.textBaseline = 'middle';

		for (let i = 0; i < n; i++) {
			const pos = labelSpace + (i + 0.5) * cellSize;
			ctx.fillStyle = witnessSet.has(i) ? '#ef4444' : '#8888a0';
			// Top labels
			ctx.fillText(String(i), pos, labelSpace / 2);
			// Left labels
			ctx.fillText(String(i), labelSpace / 2, pos);
		}
	});
</script>

<canvas bind:this={canvas} class="matrix-canvas"></canvas>

<style>
	.matrix-canvas {
		border-radius: 0.5rem;
		border: 1px solid var(--color-border);
	}
</style>
