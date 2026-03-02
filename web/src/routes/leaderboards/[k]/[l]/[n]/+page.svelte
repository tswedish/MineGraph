<script lang="ts">
	import { page } from '$app/state';
	import { onDestroy } from 'svelte';
	import { getLeaderboard, connectEvents, type LeaderboardDetail, type EventMessage } from '$lib/api';
	import MatrixView from '$lib/components/MatrixView.svelte';
	import CircleLayout from '$lib/components/CircleLayout.svelte';

	let detail = $state<LeaderboardDetail | null>(null);
	let loading = $state(true);
	let error = $state('');
	let flashCids = $state<Set<string>>(new Set());

	// Track previous CIDs to detect new entries on refresh
	let prevCids = new Set<string>();

	function refresh(k: number, l: number, n: number) {
		getLeaderboard(k, l, n)
			.then((data) => {
				// Detect new or rank-changed entries
				const newFlash = new Set<string>();
				for (const entry of data.entries) {
					if (!prevCids.has(entry.graph_cid)) {
						newFlash.add(entry.graph_cid);
					}
				}

				prevCids = new Set(data.entries.map((e) => e.graph_cid));
				detail = data;

				if (newFlash.size > 0) {
					flashCids = newFlash;
					setTimeout(() => { flashCids = new Set(); }, 1500);
				}
			})
			.catch((e) => {
				error = e instanceof Error ? e.message : 'Failed to load leaderboard';
			})
			.finally(() => {
				loading = false;
			});
	}

	// Initial load + reload on param change
	$effect(() => {
		const k = Number(page.params.k);
		const l = Number(page.params.l);
		const n = Number(page.params.n);
		loading = true;
		error = '';
		detail = null;
		prevCids = new Set();
		flashCids = new Set();

		refresh(k, l, n);
	});

	// WebSocket subscription for live updates
	let ws: WebSocket | null = null;

	$effect(() => {
		const k = Number(page.params.k);
		const l = Number(page.params.l);
		const n = Number(page.params.n);

		// Clean up previous connection
		if (ws) {
			ws.onclose = null;
			ws.close();
			ws = null;
		}

		const socket = connectEvents();
		ws = socket;

		socket.onmessage = (ev: MessageEvent) => {
			try {
				const msg: EventMessage = JSON.parse(ev.data);
				if (msg.event_type === 'leaderboard.admitted') {
					const payload = JSON.parse(msg.payload);
					// Only refresh if this event matches our leaderboard
					if (payload.k === k && payload.ell === l && payload.n === n) {
						refresh(k, l, n);
					}
				}
			} catch {
				// ignore malformed messages
			}
		};

		socket.onerror = () => socket.close();

		return () => {
			socket.onclose = null;
			socket.close();
		};
	});

	onDestroy(() => {
		if (ws) {
			ws.onclose = null;
			ws.close();
			ws = null;
		}
	});
</script>

<svelte:head>
	<title>{detail ? `R(${detail.k},${detail.ell}) n=${detail.n}` : 'Leaderboard'} — RamseyNet</title>
</svelte:head>

<div class="page">
	{#if loading}
		<div class="loading">Loading leaderboard...</div>
	{:else if error}
		<div class="error">{error}</div>
	{:else if detail}
		<a href="/leaderboards" class="back-link">Leaderboards</a>

		<h1>R({detail.k},{detail.ell}) <span class="n-label">n={detail.n}</span></h1>
		<p class="subtitle">{detail.entries.length} ranked {detail.entries.length === 1 ? 'entry' : 'entries'}</p>

		{#if detail.top_graph}
			<section class="viz-section">
				<h2>Top Graph</h2>
				<div class="viz-row">
					<MatrixView rgxf={detail.top_graph} size={360} />
					<CircleLayout rgxf={detail.top_graph} size={360} />
				</div>
			</section>
		{/if}

		{#if detail.entries.length > 0}
			<section class="table-section">
				<table>
					<thead>
						<tr>
							<th>#</th>
							<th>CID</th>
							<th>C<sub>max</sub></th>
							<th>C<sub>min</sub></th>
							<th>|Aut|</th>
							<th>Admitted</th>
						</tr>
					</thead>
					<tbody>
						{#each detail.entries as entry (entry.graph_cid)}
							<tr class:rank1={entry.rank === 1} class:flash={flashCids.has(entry.graph_cid)}>
								<td class="rank">{entry.rank}</td>
								<td class="cid">
									<a href="/submissions/{entry.graph_cid}">{entry.graph_cid.slice(0, 16)}...</a>
								</td>
								<td class="score">{entry.tier1_max}</td>
								<td class="score">{entry.tier1_min}</td>
								<td class="score">{entry.tier2_aut}</td>
								<td class="timestamp">{new Date(entry.admitted_at).toLocaleString()}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</section>
		{:else}
			<div class="empty">No entries yet. Submit a graph to be the first!</div>
		{/if}
	{/if}
</div>

<style>
	.page {
		max-width: 900px;
	}

	.loading, .error {
		padding: 2rem;
		text-align: center;
		color: var(--color-text-muted);
		font-size: 0.875rem;
	}

	.error {
		color: var(--color-rejected);
	}

	.back-link {
		font-size: 0.8125rem;
		color: var(--color-text-muted);
		display: inline-block;
		margin-bottom: 0.75rem;
	}

	.back-link::before {
		content: '\2190 ';
	}

	.back-link:hover {
		color: var(--color-accent);
	}

	h1 {
		font-family: var(--font-mono);
		font-size: 2rem;
		font-weight: 700;
	}

	.n-label {
		color: var(--color-accepted);
	}

	.subtitle {
		color: var(--color-text-muted);
		font-size: 0.875rem;
		margin-bottom: 1.5rem;
	}

	h2 {
		font-family: var(--font-mono);
		font-size: 1.125rem;
		font-weight: 600;
		margin-bottom: 1rem;
	}

	.viz-section {
		margin-bottom: 2rem;
		padding-bottom: 2rem;
		border-bottom: 1px solid var(--color-border);
	}

	.viz-row {
		display: flex;
		flex-wrap: wrap;
		gap: 1.5rem;
	}

	.table-section {
		margin-top: 1rem;
	}

	table {
		width: 100%;
		border-collapse: collapse;
	}

	th {
		text-align: left;
		font-family: var(--font-mono);
		font-size: 0.75rem;
		font-weight: 600;
		color: var(--color-text-muted);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		padding: 0.5rem 0.75rem;
		border-bottom: 1px solid var(--color-border);
	}

	td {
		padding: 0.625rem 0.75rem;
		font-size: 0.875rem;
		border-bottom: 1px solid var(--color-border);
	}

	.rank {
		font-family: var(--font-mono);
		font-weight: 700;
		color: var(--color-text-muted);
		width: 2rem;
	}

	tr.rank1 .rank {
		color: var(--color-accepted);
	}

	.cid {
		font-family: var(--font-mono);
		font-size: 0.75rem;
	}

	.cid a {
		color: var(--color-text);
		text-decoration: none;
	}

	.cid a:hover {
		color: var(--color-accent);
	}

	.score {
		font-family: var(--font-mono);
		font-size: 0.8125rem;
	}

	.timestamp {
		font-size: 0.8125rem;
		color: var(--color-text-muted);
	}

	.empty {
		padding: 2rem;
		text-align: center;
		color: var(--color-text-muted);
		font-size: 0.875rem;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 0.75rem;
	}

	/* Flash animation for newly admitted entries */
	@keyframes flash-row {
		0% { background-color: transparent; }
		20% { background-color: rgba(46, 204, 113, 0.25); }
		100% { background-color: transparent; }
	}

	tr.flash {
		animation: flash-row 1.5s ease-out;
	}
</style>
