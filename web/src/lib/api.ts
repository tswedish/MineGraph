const BASE = '/api';

// ── Types ────────────────────────────────────────────────────────────

export interface HealthResponse {
	name: string;
	version: string;
	status: string;
}

export interface RgxfJson {
	n: number;
	encoding: string;
	bits_b64: string;
}

export interface VerifyRequest {
	oras_version: string;
	k: number;
	ell: number;
	graph: RgxfJson;
	want_cid: boolean;
}

export interface VerifyResponse {
	status: 'accepted' | 'rejected';
	graph_cid?: string;
	reason?: string;
	witness?: number[];
}

export interface Challenge {
	challenge_id: string;
	k: number;
	ell: number;
	description: string;
	created_at: string;
}

export interface Record {
	challenge_id: string;
	best_n: number;
	best_cid: string;
	updated_at: string;
}

export interface SubmitRequest {
	challenge_id: string;
	graph: RgxfJson;
}

export interface SubmitResponse {
	graph_cid: string;
	verdict: 'accepted' | 'rejected';
	reason?: string;
	witness?: number[];
	is_new_record: boolean;
}

export interface EventMessage {
	seq: number;
	event_type: string;
	payload: string;
	created_at: string;
}

// ── API Functions ────────────────────────────────────────────────────

export async function getHealth(): Promise<HealthResponse> {
	const res = await fetch(`${BASE}/health`);
	return res.json();
}

export async function getChallenges(): Promise<Challenge[]> {
	const res = await fetch(`${BASE}/challenges`);
	const data = await res.json();
	return data.challenges;
}

export async function createChallenge(
	k: number,
	ell: number,
	description: string
): Promise<Challenge> {
	const res = await fetch(`${BASE}/challenges`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ k, ell, description })
	});
	if (!res.ok) {
		const err = await res.json();
		throw new Error(err.error || `HTTP ${res.status}`);
	}
	const data = await res.json();
	return data.challenge;
}

export async function getChallenge(
	id: string
): Promise<{ challenge: Challenge; record: Record | null }> {
	const res = await fetch(`${BASE}/challenges/${encodeURIComponent(id)}`);
	if (!res.ok) {
		const err = await res.json();
		throw new Error(err.error || `HTTP ${res.status}`);
	}
	return res.json();
}

export async function getRecords(): Promise<Record[]> {
	const res = await fetch(`${BASE}/records`);
	const data = await res.json();
	return data.records;
}

export async function submitVerify(req: VerifyRequest): Promise<VerifyResponse> {
	const res = await fetch(`${BASE}/verify`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(req)
	});
	if (!res.ok) {
		const err = await res.json();
		throw new Error(err.error || `HTTP ${res.status}`);
	}
	return res.json();
}

export async function submitGraph(req: SubmitRequest): Promise<SubmitResponse> {
	const res = await fetch(`${BASE}/submit`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(req)
	});
	if (!res.ok) {
		const err = await res.json();
		throw new Error(err.error || `HTTP ${res.status}`);
	}
	return res.json();
}

// ── WebSocket ────────────────────────────────────────────────────────

/**
 * Connect to the OESP-1 event stream.
 * Returns a WebSocket that emits JSON `EventMessage` frames.
 */
export function connectEvents(afterSeq = 0): WebSocket {
	const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
	const ws = new WebSocket(`${protocol}//${window.location.host}${BASE}/events`);
	ws.onopen = () => {
		if (afterSeq > 0) {
			ws.send(JSON.stringify({ after_seq: afterSeq }));
		}
	};
	return ws;
}
