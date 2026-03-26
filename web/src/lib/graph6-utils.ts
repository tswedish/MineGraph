// graph6 encoding and random graph generation

/** Encode a symmetric boolean adjacency matrix as graph6. */
export function encodeGraph6(matrix: boolean[][]): string {
	const n = matrix.length;
	if (n === 0 || n >= 63) return '';

	// Collect bits: upper triangle, column-major (j=1..n-1, i=0..j-1)
	const bits: boolean[] = [];
	for (let j = 1; j < n; j++) {
		for (let i = 0; i < j; i++) {
			bits.push(matrix[i][j]);
		}
	}

	// Pad to multiple of 6
	while (bits.length % 6 !== 0) {
		bits.push(false);
	}

	// Pack into 6-bit groups → characters
	let result = String.fromCharCode(n + 63);
	for (let i = 0; i < bits.length; i += 6) {
		let val = 0;
		for (let k = 0; k < 6; k++) {
			if (bits[i + k]) val |= (1 << (5 - k));
		}
		result += String.fromCharCode(val + 63);
	}
	return result;
}

/** Generate a random graph on n vertices (edge probability 0.5). Returns graph6 string. */
export function randomGraph(n: number): string {
	const matrix: boolean[][] = Array.from({ length: n }, () => Array(n).fill(false));
	for (let i = 0; i < n; i++) {
		for (let j = i + 1; j < n; j++) {
			const edge = Math.random() < 0.5;
			matrix[i][j] = edge;
			matrix[j][i] = edge;
		}
	}
	return encodeGraph6(matrix);
}

/** Extract n (vertex count) from a graph6 string without full decode. */
export function graph6VertexCount(g6: string): number | null {
	if (!g6 || g6.length === 0) return null;
	const code = g6.charCodeAt(0);
	if (code < 63 || code > 126) return null;
	return code - 63;
}
