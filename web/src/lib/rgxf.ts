import type { RgxfJson } from './api';

/**
 * Decode an RGXF base64 packed upper-triangular bitstring into
 * a symmetric boolean adjacency matrix.
 *
 * Bit layout matches Rust's AdjacencyMatrix: upper-triangular pairs
 * (0,1),(0,2),...,(0,n-1),(1,2),...,(n-2,n-1) packed MSB-first.
 */
export function decodeRgxf(rgxf: RgxfJson): boolean[][] {
	const n = rgxf.n;
	const matrix: boolean[][] = Array.from({ length: n }, () => Array(n).fill(false));

	if (n < 2) return matrix;

	const bytes = base64ToBytes(rgxf.bits_b64);
	let bitIndex = 0;

	for (let i = 0; i < n; i++) {
		for (let j = i + 1; j < n; j++) {
			const byteIdx = Math.floor(bitIndex / 8);
			const bitIdx = 7 - (bitIndex % 8); // MSB-first
			if (byteIdx < bytes.length && (bytes[byteIdx] & (1 << bitIdx)) !== 0) {
				matrix[i][j] = true;
				matrix[j][i] = true;
			}
			bitIndex++;
		}
	}

	return matrix;
}

/** Check if edge (i,j) exists in decoded matrix. */
export function hasEdge(matrix: boolean[][], i: number, j: number): boolean {
	if (i < 0 || j < 0 || i >= matrix.length || j >= matrix.length) return false;
	return matrix[i][j];
}

/** Count total edges in the graph. */
export function edgeCount(matrix: boolean[][]): number {
	const n = matrix.length;
	let count = 0;
	for (let i = 0; i < n; i++) {
		for (let j = i + 1; j < n; j++) {
			if (matrix[i][j]) count++;
		}
	}
	return count;
}

function base64ToBytes(b64: string): Uint8Array {
	const binary = atob(b64);
	const bytes = new Uint8Array(binary.length);
	for (let i = 0; i < binary.length; i++) {
		bytes[i] = binary.charCodeAt(i);
	}
	return bytes;
}
