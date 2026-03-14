<script lang="ts">
	import { untrack } from 'svelte';
	import { submitGraph, type RgxfJson, type SubmitResponse } from '$lib/api';
	import MatrixView from './MatrixView.svelte';

	let props: { k?: number; ell?: number } = $props();

	// Seed form state from props — intentionally captured once (not reactive).
	let k = $state(untrack(() => props.k ?? 3));
	let ell = $state(untrack(() => props.ell ?? 3));
	let n = $state(0);
	let rgxfInput = $state('');
	let parsedRgxf = $state<RgxfJson | null>(null);
	let parseError = $state('');
	let submitting = $state(false);
	let result = $state<SubmitResponse | null>(null);
	let submitError = $state('');

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
			// Auto-fill n from RGXF
			n = obj.n;
		} catch (e) {
			parseError = 'Invalid JSON';
		}
	});

	async function handleSubmit() {
		if (!k || !ell || !n || !parsedRgxf) return;
		submitting = true;
		result = null;
		submitError = '';

		try {
			result = await submitGraph({ k, ell, n, graph: parsedRgxf });
		} catch (e) {
			submitError = e instanceof Error ? e.message : 'Submission failed';
		} finally {
			submitting = false;
		}
	}
</script>

<div class="submit-form">
	<div class="params-row">
		<div class="field">
			<label for="k-input">K</label>
			<input id="k-input" type="number" min="2" bind:value={k} />
		</div>
		<div class="field">
			<label for="ell-input">L</label>
			<input id="ell-input" type="number" min="2" bind:value={ell} />
		</div>
		<div class="field">
			<label for="n-input">N</label>
			<input id="n-input" type="number" min="1" bind:value={n} />
		</div>
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
		disabled={!k || !ell || !n || !parsedRgxf || submitting}
	>
		{submitting ? 'Submitting...' : 'Submit Graph'}
	</button>

	{#if result}
		<div class="result" class:accepted={result.verdict === 'accepted'} class:rejected={result.verdict === 'rejected'}>
			<div class="verdict-badge">{result.verdict}</div>
			{#if result.reason}<p class="reason">{result.reason}</p>{/if}
			{#if result.admitted}<p class="new-record">Admitted to leaderboard! Rank #{result.rank}</p>{/if}
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

	.params-row {
		display: flex;
		gap: 1rem;
	}

	.params-row .field {
		flex: 1;
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

	input, textarea {
		background: var(--color-bg);
		border: 1px solid var(--color-border);
		border-radius: 0.5rem;
		color: var(--color-text);
		font-family: var(--font-mono);
		font-size: 0.8125rem;
		padding: 0.625rem 0.75rem;
	}

	input:focus, textarea:focus {
		outline: none;
		border-color: var(--color-accent);
	}

	input[type="number"] {
		width: 100%;
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
