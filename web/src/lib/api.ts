const BASE = '/api';

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

export async function getHealth(): Promise<HealthResponse> {
	const res = await fetch(`${BASE}/health`);
	return res.json();
}

export async function getChallenges(): Promise<unknown[]> {
	const res = await fetch(`${BASE}/challenges`);
	const data = await res.json();
	return data.challenges;
}

export async function getRecords(): Promise<unknown[]> {
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
