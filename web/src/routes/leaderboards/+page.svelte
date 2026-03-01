<script lang="ts">
	import { getLeaderboards, type LeaderboardSummary } from '$lib/api';

	let summaries = $state<LeaderboardSummary[]>([]);
	let loading = $state(true);
	let error = $state('');

	// Group by (k, ell) pair
	const grouped = $derived(() => {
		const map = new Map<string, LeaderboardSummary[]>();
		for (const s of summaries) {
			const key = `${s.k},${s.ell}`;
			if (!map.has(key)) map.set(key, []);
			map.get(key)!.push(s);
		}
		return map;
	});

	$effect(() => {
		let cancelled = false;
		getLeaderboards()
			.then((data) => {
				if (cancelled) return;
				summaries = data;
			})
			.catch((e) => {
				if (cancelled) return;
				error = e instanceof Error ? e.message : 'Failed to load';
			})
			.finally(() => {
				if (!cancelled) loading = false;
			});
		return () => { cancelled = true; };
	});
</script>

<svelte:head>
	<title>Leaderboards — RamseyNet</title>
</svelte:head>

<div class="page">
	<h1>Leaderboards</h1>
	<p class="subtitle">Ranked Ramsey graph discoveries by (K, L, n) triple.</p>

	{#if loading}
		<div class="loading">Loading leaderboards...</div>
	{:else if error}
		<div class="error">{error}</div>
	{:else if summaries.length === 0}
		<div class="empty">No leaderboards yet. Submit a graph to create the first one.</div>
	{:else}
		<div class="pair-list">
			{#each [...grouped().entries()] as [pair, entries] (pair)}
				{@const k = entries[0].k}
				{@const ell = entries[0].ell}
				<div class="pair-card">
					<div class="pair-header">
						<span class="ramsey-label">R({k},{ell})</span>
					</div>
					<div class="n-list">
						{#each entries.sort((a, b) => a.n - b.n) as entry (entry.n)}
							<a href="/leaderboards/{k}/{ell}/{entry.n}" class="n-chip">
								<span class="n-value">n={entry.n}</span>
								<span class="n-count">{entry.entry_count} {entry.entry_count === 1 ? 'entry' : 'entries'}</span>
							</a>
						{/each}
					</div>
				</div>
			{/each}
		</div>
	{/if}
</div>

<style>
	.page {
		max-width: 800px;
	}

	h1 {
		font-family: var(--font-mono);
		font-size: 1.75rem;
		font-weight: 700;
		margin-bottom: 0.5rem;
	}

	.subtitle {
		color: var(--color-text-muted);
		font-size: 0.875rem;
		margin-bottom: 2rem;
	}

	.loading, .error, .empty {
		padding: 2rem;
		text-align: center;
		color: var(--color-text-muted);
		font-size: 0.875rem;
	}

	.error {
		color: var(--color-rejected);
	}

	.pair-list {
		display: flex;
		flex-direction: column;
		gap: 1.25rem;
	}

	.pair-card {
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 0.75rem;
		padding: 1.25rem;
	}

	.pair-header {
		margin-bottom: 0.75rem;
	}

	.ramsey-label {
		font-family: var(--font-mono);
		font-size: 1.25rem;
		font-weight: 700;
		color: var(--color-accent);
	}

	.n-list {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
	}

	.n-chip {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.125rem;
		padding: 0.5rem 0.875rem;
		background: var(--color-bg);
		border: 1px solid var(--color-border);
		border-radius: 0.5rem;
		color: var(--color-text);
		text-decoration: none;
		transition: border-color 0.15s;
	}

	.n-chip:hover {
		border-color: var(--color-accent);
	}

	.n-value {
		font-family: var(--font-mono);
		font-size: 1rem;
		font-weight: 600;
	}

	.n-count {
		font-size: 0.6875rem;
		color: var(--color-text-muted);
	}
</style>
