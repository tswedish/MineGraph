// Browser-side Ed25519 key management and signing for Extremal

import nacl from 'tweetnacl';
// @ts-ignore — noble-hashes v2 exports use .js suffix
import { blake3 } from '@noble/hashes/blake3.js';

const STORAGE_KEY = 'extremal_identity';

export interface KeyFile {
	key_id: string;
	public_key: string;
	secret_key: string;
	display_name?: string;
}

/** Convert bytes to hex string. */
export function bytesToHex(bytes: Uint8Array): string {
	return Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join('');
}

/** Convert hex string to bytes. */
export function hexToBytes(hex: string): Uint8Array {
	const bytes = new Uint8Array(hex.length / 2);
	for (let i = 0; i < bytes.length; i++) {
		bytes[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
	}
	return bytes;
}

/** Compute key_id from public key bytes: first 16 hex chars of blake3(pubkey). */
function computeKeyId(publicKey: Uint8Array): string {
	const hash = blake3(publicKey);
	return bytesToHex(hash.slice(0, 8));
}

/** Generate a new Ed25519 keypair. Returns a KeyFile object. */
export function generateKeyPair(displayName?: string): KeyFile {
	const kp = nacl.sign.keyPair();
	const publicKey = bytesToHex(kp.publicKey);
	// tweetnacl secretKey is 64 bytes (seed + publicKey), store only the 32-byte seed
	const secretKey = bytesToHex(kp.secretKey.slice(0, 32));
	const keyId = computeKeyId(kp.publicKey);

	return {
		key_id: keyId,
		public_key: publicKey,
		secret_key: secretKey,
		display_name: displayName,
	};
}

/** Build the canonical payload for signing: 4-byte LE n + graph6 bytes. */
export function canonicalPayload(n: number, graph6: string): Uint8Array {
	const encoder = new TextEncoder();
	const g6Bytes = encoder.encode(graph6);
	const buf = new Uint8Array(4 + g6Bytes.length);
	// n as u32 little-endian
	buf[0] = n & 0xff;
	buf[1] = (n >> 8) & 0xff;
	buf[2] = (n >> 16) & 0xff;
	buf[3] = (n >> 24) & 0xff;
	buf.set(g6Bytes, 4);
	return buf;
}

/** Sign a payload with the secret key (32-byte seed hex). Returns signature as hex. */
export function sign(payload: Uint8Array, secretKeyHex: string, publicKeyHex: string): string {
	const seed = hexToBytes(secretKeyHex);
	const pubKey = hexToBytes(publicKeyHex);
	// Reconstruct tweetnacl's 64-byte secret key: seed + publicKey
	const fullSecret = new Uint8Array(64);
	fullSecret.set(seed, 0);
	fullSecret.set(pubKey, 32);
	const sig = nacl.sign.detached(payload, fullSecret);
	return bytesToHex(sig);
}

/** Validate that a hex string is the expected byte length. */
export function isValidHex(hex: string, expectedBytes: number): boolean {
	if (hex.length !== expectedBytes * 2) return false;
	return /^[0-9a-fA-F]+$/.test(hex);
}

/** Parse a key.json string into a KeyFile, validating fields. */
export function parseKeyFile(json: string): KeyFile {
	const obj = JSON.parse(json);
	if (!obj.public_key || !obj.secret_key) {
		throw new Error('key.json must have public_key and secret_key fields');
	}
	if (!isValidHex(obj.public_key, 32)) {
		throw new Error('public_key must be 64 hex chars (32 bytes)');
	}
	if (!isValidHex(obj.secret_key, 32)) {
		throw new Error('secret_key must be 64 hex chars (32 bytes)');
	}
	// Recompute key_id from public key
	const pubBytes = hexToBytes(obj.public_key);
	const keyId = computeKeyId(pubBytes);
	return {
		key_id: obj.key_id || keyId,
		public_key: obj.public_key,
		secret_key: obj.secret_key,
		display_name: obj.display_name,
	};
}

/** Save identity to localStorage. */
export function saveIdentity(key: KeyFile): void {
	try {
		localStorage.setItem(STORAGE_KEY, JSON.stringify(key));
	} catch { /* localStorage may be unavailable */ }
}

/** Load identity from localStorage. Returns null if not found. */
export function loadIdentity(): KeyFile | null {
	try {
		const raw = localStorage.getItem(STORAGE_KEY);
		if (!raw) return null;
		return parseKeyFile(raw);
	} catch {
		return null;
	}
}

/** Clear saved identity from localStorage. */
export function clearIdentity(): void {
	try {
		localStorage.removeItem(STORAGE_KEY);
	} catch { /* ignore */ }
}

/** Format a KeyFile as pretty JSON for display/copy. */
export function formatKeyJson(key: KeyFile): string {
	return JSON.stringify({
		key_id: key.key_id,
		public_key: key.public_key,
		secret_key: key.secret_key,
		...(key.display_name ? { display_name: key.display_name } : {}),
	}, null, 2);
}
