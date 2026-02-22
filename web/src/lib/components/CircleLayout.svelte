<script lang="ts">
	import type { RgxfJson } from '$lib/api';
	import { decodeRgxf } from '$lib/rgxf';

	let { rgxf, witness = [], size = 400 }: { rgxf: RgxfJson; witness?: number[]; size?: number } =
		$props();

	const witnessSet = $derived(new Set(witness));
	const matrix = $derived(decodeRgxf(rgxf));

	const padding = 40;
	const radius = $derived((size - padding * 2) / 2);
	const center = $derived(size / 2);

	interface Vertex {
		i: number;
		x: number;
		y: number;
	}

	const vertices = $derived.by<Vertex[]>(() => {
		const n = rgxf.n;
		return Array.from({ length: n }, (_, i) => {
			const angle = (2 * Math.PI * i) / n - Math.PI / 2;
			return {
				i,
				x: center + radius * Math.cos(angle),
				y: center + radius * Math.sin(angle)
			};
		});
	});

	interface Edge {
		x1: number;
		y1: number;
		x2: number;
		y2: number;
		isWitness: boolean;
	}

	const edges = $derived.by<Edge[]>(() => {
		const result: Edge[] = [];
		const n = rgxf.n;
		for (let i = 0; i < n; i++) {
			for (let j = i + 1; j < n; j++) {
				if (matrix[i][j]) {
					result.push({
						x1: vertices[i].x,
						y1: vertices[i].y,
						x2: vertices[j].x,
						y2: vertices[j].y,
						isWitness: witnessSet.has(i) && witnessSet.has(j)
					});
				}
			}
		}
		return result;
	});

	const vertexRadius = $derived(Math.max(3, Math.min(8, 200 / Math.max(rgxf.n, 1))));
	const fontSize = $derived(Math.max(8, Math.min(12, 300 / Math.max(rgxf.n, 1))));
</script>

<svg viewBox="0 0 {size} {size}" width={size} height={size} class="circle-layout">
	{#each edges as edge}
		<line
			x1={edge.x1}
			y1={edge.y1}
			x2={edge.x2}
			y2={edge.y2}
			stroke={edge.isWitness ? '#ef4444' : '#4a4a6a'}
			stroke-width={edge.isWitness ? 1.5 : 0.5}
			stroke-opacity={edge.isWitness ? 0.9 : 0.4}
		/>
	{/each}
	{#each vertices as v}
		<circle
			cx={v.x}
			cy={v.y}
			r={vertexRadius}
			fill={witnessSet.has(v.i) ? '#ef4444' : '#6366f1'}
		/>
		<text
			x={v.x}
			y={v.y - vertexRadius - 4}
			text-anchor="middle"
			fill={witnessSet.has(v.i) ? '#ef4444' : '#8888a0'}
			font-size={fontSize}
			font-family="monospace"
		>
			{v.i}
		</text>
	{/each}
</svg>

<style>
	.circle-layout {
		border-radius: 0.5rem;
		border: 1px solid var(--color-border);
		background: var(--color-surface);
	}
</style>
