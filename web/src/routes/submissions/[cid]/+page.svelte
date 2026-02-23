<script lang="ts">
	import { page } from '$app/state';
	import { getSubmission, type SubmissionDetail, type RgxfJson } from '$lib/api';
	import MatrixView from '$lib/components/MatrixView.svelte';
	import CircleLayout from '$lib/components/CircleLayout.svelte';

	let detail = $state<SubmissionDetail | null>(null);
	let loading = $state(true);
	let error = $state('');

	$effect(() => {
		const cid = page.params.cid;
		loading = true;
		error = '';
		detail = null;

		let cancelled = false;

		getSubmission(cid)
			.then((data) => {
				if (cancelled) return;
				detail = data;
			})
			.catch((e) => {
				if (cancelled) return;
				error = e instanceof Error ? e.message : 'Failed to load submission';
			})
			.finally(() => {
				if (!cancelled) loading = false;
			});

		return () => { cancelled = true; };
	});

	const challengeLabel = $derived(
		detail?.challenge
			? `R(${detail.challenge.k},${detail.challenge.ell})`
			: detail?.challenge_id ?? ''
	);
</script>

<svelte:head>
	<title>Submission {detail?.graph_cid?.slice(0, 12) ?? ''} — RamseyNet</title>
</svelte:head>

<div class="page">
	{#if loading}
		<div class="loading">Loading submission...</div>
	{:else if error}
		<div class="error">{error}</div>
	{:else if detail}
		<button class="back-link" onclick={() => history.back()}>Back</button>

		<h1 class="cid-header">{detail.graph_cid}</h1>

		<div class="meta">
			<div class="meta-row">
				<span class="meta-label">Challenge</span>
				<a href="/challenges/{encodeURIComponent(detail.challenge_id)}" class="meta-link">
					{detail.challenge_id} — {challengeLabel}
				</a>
			</div>
			<div class="meta-row">
				<span class="meta-label">Graph Size</span>
				<span class="meta-value">n = {detail.n}</span>
			</div>
			<div class="meta-row">
				<span class="meta-label">Verdict</span>
				<span class="verdict-badge" class:accepted={detail.verdict === 'accepted'} class:rejected={detail.verdict === 'rejected'}>
					{detail.verdict ?? 'pending'}
				</span>
				{#if detail.is_record}
					<span class="record-badge">Current Record</span>
				{/if}
			</div>
			{#if detail.reason}
				<div class="meta-row">
					<span class="meta-label">Reason</span>
					<span class="meta-value">{detail.reason}</span>
				</div>
			{/if}
			{#if detail.witness && detail.witness.length > 0}
				<div class="meta-row">
					<span class="meta-label">Witness</span>
					<span class="meta-value mono">[{detail.witness.join(', ')}]</span>
				</div>
			{/if}
			<div class="meta-row">
				<span class="meta-label">Submitted</span>
				<span class="meta-value">{new Date(detail.submitted_at).toLocaleString()}</span>
			</div>
			{#if detail.verified_at}
				<div class="meta-row">
					<span class="meta-label">Verified</span>
					<span class="meta-value">{new Date(detail.verified_at).toLocaleString()}</span>
				</div>
			{/if}
		</div>

		{#if detail.rgxf}
			<section class="viz-section">
				<h2>Graph Visualization</h2>
				<div class="viz-row">
					<MatrixView rgxf={detail.rgxf} witness={detail.witness ?? []} size={360} />
					<CircleLayout rgxf={detail.rgxf} witness={detail.witness ?? []} size={360} />
				</div>
			</section>
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
		cursor: pointer;
		background: none;
		border: none;
		padding: 0;
		font: inherit;
	}

	.back-link::before {
		content: '\2190 ';
	}

	.back-link:hover {
		color: var(--color-accent);
	}

	.cid-header {
		font-family: var(--font-mono);
		font-size: 1rem;
		font-weight: 700;
		word-break: break-all;
		line-height: 1.4;
		margin-bottom: 1.5rem;
	}

	.meta {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	.meta-row {
		display: flex;
		align-items: baseline;
		gap: 1rem;
	}

	.meta-label {
		font-family: var(--font-mono);
		font-size: 0.6875rem;
		color: var(--color-text-muted);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		min-width: 7rem;
		flex-shrink: 0;
	}

	.meta-value {
		font-size: 0.875rem;
	}

	.meta-value.mono {
		font-family: var(--font-mono);
		font-size: 0.8125rem;
	}

	.meta-link {
		font-family: var(--font-mono);
		font-size: 0.8125rem;
	}

	.verdict-badge {
		font-family: var(--font-mono);
		font-size: 0.75rem;
		font-weight: 700;
		text-transform: uppercase;
		padding: 0.2rem 0.6rem;
		border-radius: 0.25rem;
		letter-spacing: 0.05em;
	}

	.verdict-badge.accepted {
		color: var(--color-accepted);
		background: color-mix(in srgb, var(--color-accepted) 15%, transparent);
	}

	.record-badge {
		font-family: var(--font-mono);
		font-size: 0.75rem;
		font-weight: 700;
		text-transform: uppercase;
		padding: 0.2rem 0.6rem;
		border-radius: 0.25rem;
		letter-spacing: 0.05em;
		color: #f59e0b;
		background: color-mix(in srgb, #f59e0b 15%, transparent);
	}

	.verdict-badge.rejected {
		color: var(--color-rejected);
		background: color-mix(in srgb, var(--color-rejected) 15%, transparent);
	}

	.viz-section {
		margin-top: 2rem;
		padding-top: 2rem;
		border-top: 1px solid var(--color-border);
	}

	h2 {
		font-family: var(--font-mono);
		font-size: 1.125rem;
		font-weight: 600;
		margin-bottom: 1rem;
	}

	.viz-row {
		display: flex;
		flex-wrap: wrap;
		gap: 1.5rem;
	}
</style>
