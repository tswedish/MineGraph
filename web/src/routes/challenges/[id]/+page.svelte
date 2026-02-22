<script lang="ts">
	import { page } from '$app/state';
	import { getChallenge, type Challenge, type Record, type RgxfJson } from '$lib/api';
	import MatrixView from '$lib/components/MatrixView.svelte';
	import CircleLayout from '$lib/components/CircleLayout.svelte';
	import SubmitForm from '$lib/components/SubmitForm.svelte';

	let challenge = $state<Challenge | null>(null);
	let record = $state<Record | null>(null);
	let recordGraph = $state<RgxfJson | null>(null);
	let loading = $state(true);
	let error = $state('');

	$effect(() => {
		const id = page.params.id;
		loading = true;
		error = '';
		challenge = null;
		record = null;
		recordGraph = null;

		let cancelled = false;

		getChallenge(id)
			.then((data) => {
				if (cancelled) return;
				challenge = data.challenge;
				record = data.record;
				// Server may include record_graph (added in Phase 4)
				recordGraph = (data as any).record_graph ?? null;
			})
			.catch((e) => {
				if (cancelled) return;
				error = e instanceof Error ? e.message : 'Failed to load challenge';
			})
			.finally(() => {
				if (!cancelled) loading = false;
			});

		return () => { cancelled = true; };
	});
</script>

<svelte:head>
	<title>{challenge ? `R(${challenge.k},${challenge.ell})` : 'Challenge'} — RamseyNet</title>
</svelte:head>

<div class="page">
	{#if loading}
		<div class="loading">Loading challenge...</div>
	{:else if error}
		<div class="error">{error}</div>
	{:else if challenge}
		<div class="header">
			<a href="/challenges" class="back-link">Challenges</a>
			<h1>R({challenge.k},{challenge.ell})</h1>
			<div class="challenge-id">{challenge.challenge_id}</div>
			{#if challenge.description}
				<p class="description">{challenge.description}</p>
			{/if}
		</div>

		<section class="record-section">
			<h2>Current Record</h2>
			{#if record}
				<div class="record-info">
					<div class="record-stat">
						<span class="stat-label">Best n</span>
						<span class="stat-value">{record.best_n}</span>
					</div>
					<div class="record-stat">
						<span class="stat-label">Graph CID</span>
						<span class="stat-cid">{record.best_cid}</span>
					</div>
					<div class="record-stat">
						<span class="stat-label">Updated</span>
						<span class="stat-value">{new Date(record.updated_at).toLocaleString()}</span>
					</div>
				</div>

				{#if recordGraph}
					<div class="graph-viz">
						<h3>Record Graph (n={recordGraph.n})</h3>
						<div class="viz-row">
							<MatrixView rgxf={recordGraph} size={360} />
							<CircleLayout rgxf={recordGraph} size={360} />
						</div>
					</div>
				{/if}
			{:else}
				<div class="no-record">No submissions yet. Be the first to submit!</div>
			{/if}
		</section>

		<section class="submit-section">
			<h2>Submit a Graph</h2>
			<SubmitForm challengeId={challenge.challenge_id} />
		</section>
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

	.challenge-id {
		font-family: var(--font-mono);
		font-size: 0.8125rem;
		color: var(--color-text-muted);
		margin-bottom: 0.5rem;
	}

	.description {
		color: var(--color-text-muted);
		font-size: 0.875rem;
		margin-bottom: 1rem;
	}

	h2 {
		font-family: var(--font-mono);
		font-size: 1.125rem;
		font-weight: 600;
		margin-bottom: 1rem;
	}

	h3 {
		font-family: var(--font-mono);
		font-size: 0.875rem;
		font-weight: 600;
		margin-bottom: 0.75rem;
		color: var(--color-text-muted);
	}

	.record-section, .submit-section {
		margin-top: 2rem;
		padding-top: 2rem;
		border-top: 1px solid var(--color-border);
	}

	.record-info {
		display: flex;
		flex-wrap: wrap;
		gap: 1.5rem;
		margin-bottom: 1.5rem;
	}

	.record-stat {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}

	.stat-label {
		font-family: var(--font-mono);
		font-size: 0.6875rem;
		color: var(--color-text-muted);
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.stat-value {
		font-family: var(--font-mono);
		font-size: 1.25rem;
		font-weight: 700;
		color: var(--color-accepted);
	}

	.stat-cid {
		font-family: var(--font-mono);
		font-size: 0.75rem;
		color: var(--color-text);
		word-break: break-all;
		max-width: 400px;
	}

	.viz-row {
		display: flex;
		flex-wrap: wrap;
		gap: 1.5rem;
	}

	.no-record {
		color: var(--color-text-muted);
		font-size: 0.875rem;
		padding: 1.5rem;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 0.75rem;
		text-align: center;
	}
</style>
