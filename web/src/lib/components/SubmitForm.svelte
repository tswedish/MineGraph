<script lang="ts">
	import { getChallenges, submitGraph, type Challenge, type RgxfJson, type SubmitResponse } from '$lib/api';
	import MatrixView from './MatrixView.svelte';

	let { challengeId = '' }: { challengeId?: string } = $props();

	let challenges = $state<Challenge[]>([]);
	let selectedChallenge = $state('');
	let rgxfInput = $state('');
	let parsedRgxf = $state<RgxfJson | null>(null);
	let parseError = $state('');
	let submitting = $state(false);
	let result = $state<SubmitResponse | null>(null);
	let submitError = $state('');

	$effect(() => {
		getChallenges().then((c) => (challenges = c)).catch(() => {});
	});

	$effect(() => {
		if (challengeId) selectedChallenge = challengeId;
	});

	$effect(() => {
		parseError = '';
		parsedRgxf = null;
		if (!rgxfInput.trim()) return;

		try {
			const obj = JSON.parse(rgxfInput);
			if (typeof obj.n !== 'number' || typeof obj.bits_b64 !== 'string') {
				parseError = 'Missing required fields: n, bits_b64';
				return;
			}
			parsedRgxf = obj as RgxfJson;
		} catch (e) {
			parseError = 'Invalid JSON';
		}
	});

	async function handleSubmit() {
		if (!selectedChallenge || !parsedRgxf) return;
		submitting = true;
		result = null;
		submitError = '';

		try {
			result = await submitGraph({
				challenge_id: selectedChallenge,
				graph: parsedRgxf
			});
		} catch (e) {
			submitError = e instanceof Error ? e.message : 'Submission failed';
		} finally {
			submitting = false;
		}
	}
</script>

<div class="submit-form">
	<div class="field">
		<label for="challenge-select">Challenge</label>
		<select id="challenge-select" bind:value={selectedChallenge}>
			<option value="">Select a challenge...</option>
			{#each challenges as c}
				<option value={c.challenge_id}>
					{c.challenge_id} — R({c.k},{c.ell})
				</option>
			{/each}
		</select>
	</div>

	<div class="field">
		<label for="rgxf-input">RGXF JSON</label>
		<textarea
			id="rgxf-input"
			bind:value={rgxfInput}
			placeholder={`{"n": 5, "encoding": "utri_b64_v1", "bits_b64": "..."}`}
			rows="6"
		></textarea>
		{#if parseError}
			<span class="error-text">{parseError}</span>
		{/if}
	</div>

	{#if parsedRgxf}
		<div class="preview">
			<span class="preview-label">Preview (n={parsedRgxf.n})</span>
			<MatrixView rgxf={parsedRgxf} size={300} />
		</div>
	{/if}

	<button
		class="submit-btn"
		onclick={handleSubmit}
		disabled={!selectedChallenge || !parsedRgxf || submitting}
	>
		{submitting ? 'Submitting...' : 'Submit Graph'}
	</button>

	{#if result}
		<div class="result" class:accepted={result.verdict === 'accepted'} class:rejected={result.verdict === 'rejected'}>
			<div class="verdict-badge">{result.verdict}</div>
			{#if result.reason}<p class="reason">{result.reason}</p>{/if}
			{#if result.is_new_record}<p class="new-record">New record!</p>{/if}
			<p class="cid">CID: {result.graph_cid}</p>
			{#if result.witness && result.witness.length > 0}
				<p class="witness-info">Witness: [{result.witness.join(', ')}]</p>
				<MatrixView rgxf={parsedRgxf!} witness={result.witness} size={300} />
			{/if}
		</div>
	{/if}

	{#if submitError}
		<div class="error-box">{submitError}</div>
	{/if}
</div>

<style>
	.submit-form {
		display: flex;
		flex-direction: column;
		gap: 1.25rem;
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
	}

	label {
		font-family: var(--font-mono);
		font-size: 0.8125rem;
		font-weight: 600;
		color: var(--color-text-muted);
	}

	select, textarea {
		background: var(--color-bg);
		border: 1px solid var(--color-border);
		border-radius: 0.5rem;
		color: var(--color-text);
		font-family: var(--font-mono);
		font-size: 0.8125rem;
		padding: 0.625rem 0.75rem;
	}

	select:focus, textarea:focus {
		outline: none;
		border-color: var(--color-accent);
	}

	textarea {
		resize: vertical;
		min-height: 100px;
	}

	.error-text {
		color: var(--color-rejected);
		font-size: 0.75rem;
	}

	.preview {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.preview-label {
		font-family: var(--font-mono);
		font-size: 0.75rem;
		color: var(--color-text-muted);
	}

	.submit-btn {
		background: var(--color-accent);
		color: white;
		border: none;
		border-radius: 0.5rem;
		padding: 0.625rem 1.5rem;
		font-size: 0.875rem;
		font-weight: 600;
		cursor: pointer;
		align-self: flex-start;
	}

	.submit-btn:hover:not(:disabled) {
		background: var(--color-accent-hover);
	}

	.submit-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.result {
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 0.75rem;
		padding: 1rem;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.result.accepted {
		border-color: var(--color-accepted);
	}

	.result.rejected {
		border-color: var(--color-rejected);
	}

	.verdict-badge {
		font-family: var(--font-mono);
		font-size: 1rem;
		font-weight: 700;
		text-transform: uppercase;
	}

	.result.accepted .verdict-badge {
		color: var(--color-accepted);
	}

	.result.rejected .verdict-badge {
		color: var(--color-rejected);
	}

	.reason {
		font-size: 0.8125rem;
		color: var(--color-text-muted);
	}

	.new-record {
		color: #f59e0b;
		font-weight: 600;
		font-size: 0.875rem;
	}

	.cid {
		font-family: var(--font-mono);
		font-size: 0.75rem;
		color: var(--color-text-muted);
		word-break: break-all;
	}

	.witness-info {
		font-family: var(--font-mono);
		font-size: 0.8125rem;
		color: var(--color-rejected);
	}

	.error-box {
		background: rgba(239, 68, 68, 0.1);
		border: 1px solid var(--color-rejected);
		border-radius: 0.5rem;
		padding: 0.75rem 1rem;
		color: var(--color-rejected);
		font-size: 0.8125rem;
	}
</style>
