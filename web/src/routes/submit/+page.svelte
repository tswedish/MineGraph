<script lang="ts">
	import SubmitForm from '$lib/components/SubmitForm.svelte';
</script>

<svelte:head>
	<title>Submit — RamseyNet</title>
</svelte:head>

<div class="page">
	<h1>Submit a Graph</h1>
	<p class="subtitle">
		Paste an RGXF-encoded graph to verify and submit for leaderboard ranking.
	</p>
	<SubmitForm />
</div>

<style>
	.page {
		max-width: 700px;
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
</style>
