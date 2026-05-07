import { installLinkInterceptor, routes } from "./router";

type StoredFile = {
	name: string;
	size: number;
	type: string;
	updatedAt: string;
	url: string;
	viewUrl: string;
	downloadUrl: string;
	inlineUrl: string;
};

const app = document.querySelector<HTMLDivElement>("#app");
const TEXT_PREVIEW_LIMIT = 1024 * 1024;

if (!app) {
	throw new Error("Missing #app root");
}

const formatBytes = (bytes: number) => {
	if (bytes === 0) {
		return "0 B";
	}

	const units = ["B", "KB", "MB", "GB", "TB"];
	const unitIndex = Math.min(
		Math.floor(Math.log(bytes) / Math.log(1024)),
		units.length - 1,
	);
	const value = bytes / 1024 ** unitIndex;
	return `${value.toFixed(value >= 10 || unitIndex === 0 ? 0 : 1)} ${units[unitIndex]}`;
};

const formatDate = (value: string) =>
	new Intl.DateTimeFormat(undefined, {
		dateStyle: "medium",
		timeStyle: "short",
	}).format(new Date(value));

const escapeHtml = (value: string) =>
	value
		.replaceAll("&", "&amp;")
		.replaceAll("<", "&lt;")
		.replaceAll(">", "&gt;")
		.replaceAll('"', "&quot;");

const decodeParam = (value = "") => {
	try {
		return decodeURIComponent(value);
	} catch {
		return value;
	}
};

const loadFiles = async () => {
	const response = await fetch("/api/files");
	if (!response.ok) {
		throw new Error("Could not load files");
	}
	return (await response.json()) as { files: StoredFile[] };
};

const isTextLike = (file: StoredFile) => {
	if (
		file.type.startsWith("text/") ||
		file.type === "application/json" ||
		file.type === "application/xml" ||
		file.type === "application/javascript" ||
		file.type === "application/typescript"
	) {
		return true;
	}

	return /\.(csv|css|html|js|json|log|md|ts|tsx|txt|xml|yaml|yml)$/i.test(
		file.name,
	);
};

const renderUnsupportedPreview = (file: StoredFile) => `
	<div class="empty-state preview-message">
		<strong>No preview available</strong>
		<span>${escapeHtml(file.name)} can still be downloaded.</span>
	</div>
`;

const renderPreview = async (file: StoredFile) => {
	if (file.type.startsWith("image/")) {
		return `<img class="media-preview" src="${file.inlineUrl}" alt="${escapeHtml(file.name)}" />`;
	}

	if (file.type.startsWith("video/")) {
		return `<video class="media-preview" src="${file.inlineUrl}" controls></video>`;
	}

	if (file.type.startsWith("audio/")) {
		return `<audio class="audio-preview" src="${file.inlineUrl}" controls></audio>`;
	}

	if (file.type === "application/pdf" || /\.pdf$/i.test(file.name)) {
		return `<iframe class="document-preview" src="${file.inlineUrl}" title="${escapeHtml(file.name)}"></iframe>`;
	}

	if (!isTextLike(file)) {
		return renderUnsupportedPreview(file);
	}

	if (file.size > TEXT_PREVIEW_LIMIT) {
		return `
			<div class="empty-state preview-message">
				<strong>Text preview is too large</strong>
				<span>${escapeHtml(file.name)} is ${formatBytes(file.size)}. Download it to view the full file.</span>
			</div>
		`;
	}

	const response = await fetch(file.inlineUrl);
	if (!response.ok) {
		throw new Error("Could not load preview");
	}

	return `<pre class="text-preview">${escapeHtml(await response.text())}</pre>`;
};

const renderFileRows = (files: StoredFile[]) => {
	if (files.length === 0) {
		return `
			<div class="empty-state">
				<strong>No files yet</strong>
				<span>Upload something to store it on this server.</span>
			</div>
		`;
	}

	return `
		<ul class="file-list">
			${files
				.map(
					(file) => `
						<li class="file-row">
							<a href="${file.viewUrl}" class="file-name" data-link>${escapeHtml(file.name)}</a>
							<span>${formatBytes(file.size)}</span>
							<span>${formatDate(file.updatedAt)}</span>
							<a href="${file.downloadUrl}" class="file-action">Download</a>
						</li>
					`,
				)
				.join("")}
		</ul>
	`;
};

const renderHome = async () => {
	app.innerHTML = `
		<main class="shell">
			<section class="upload-panel">
				<div>
					<p class="eyebrow">puppydrive</p>
					<h1>Server file drop</h1>
				</div>
				<form id="upload-form" class="upload-form">
					<label class="drop-zone">
						<input id="file-input" name="files" type="file" multiple />
						<span>Choose files</span>
					</label>
					<button type="submit">Upload</button>
				</form>
				<p id="status" class="status" aria-live="polite"></p>
			</section>
			<section class="files-panel">
				<div class="section-header">
					<h2>Files</h2>
					<button id="refresh" type="button">Refresh</button>
				</div>
				<div id="files">Loading...</div>
			</section>
		</main>
	`;

	const filesRoot = document.querySelector<HTMLDivElement>("#files");
	const status = document.querySelector<HTMLParagraphElement>("#status");
	const form = document.querySelector<HTMLFormElement>("#upload-form");
	const refresh = document.querySelector<HTMLButtonElement>("#refresh");

	const refreshFiles = async () => {
		if (!filesRoot) {
			return;
		}
		filesRoot.textContent = "Loading...";
		try {
			const { files } = await loadFiles();
			filesRoot.innerHTML = renderFileRows(files);
		} catch (error) {
			filesRoot.innerHTML = `<div class="empty-state"><strong>Could not load files</strong><span>${escapeHtml(String(error))}</span></div>`;
		}
	};

	form?.addEventListener("submit", async (event) => {
		event.preventDefault();
		if (!form || !status) {
			return;
		}

		const submitButton = form.querySelector<HTMLButtonElement>("button");
		submitButton?.setAttribute("disabled", "true");
		status.textContent = "Uploading...";

		try {
			const response = await fetch("/api/files", {
				method: "POST",
				body: new FormData(form),
			});
			const body = (await response.json()) as {
				files?: StoredFile[];
				error?: string;
			};

			if (!response.ok) {
				throw new Error(body.error ?? "Upload failed");
			}

			form.reset();
			status.textContent = `Uploaded ${body.files?.length ?? 0} file${body.files?.length === 1 ? "" : "s"}.`;
			await refreshFiles();
		} catch (error) {
			status.textContent = error instanceof Error ? error.message : "Upload failed";
		} finally {
			submitButton?.removeAttribute("disabled");
		}
	});

	refresh?.addEventListener("click", () => {
		void refreshFiles();
	});

	await refreshFiles();
};

const renderFileView = async (params: Record<string, string>) => {
	const fileName = decodeParam(params.name);
	app.innerHTML = `
		<main class="shell">
			<section class="viewer-panel">
				<div class="viewer-header">
					<a href="/" class="back-link" data-link>Back to files</a>
					<a id="download-link" class="file-action" href="#">Download</a>
				</div>
				<div id="viewer">Loading...</div>
			</section>
		</main>
	`;

	const viewer = document.querySelector<HTMLDivElement>("#viewer");
	const downloadLink = document.querySelector<HTMLAnchorElement>("#download-link");
	if (!viewer || !downloadLink) {
		return;
	}

	try {
		const { files } = await loadFiles();
		const file = files.find((candidate) => candidate.name === fileName);
		if (!file) {
			viewer.innerHTML = `
				<div class="empty-state preview-message">
					<strong>File not found</strong>
					<span>${escapeHtml(fileName || "The requested file")} is not stored on this server.</span>
				</div>
			`;
			downloadLink.remove();
			return;
		}

		downloadLink.href = file.downloadUrl;
		viewer.innerHTML = `
			<div class="file-title">
				<h1>${escapeHtml(file.name)}</h1>
				<p>${formatBytes(file.size)} · ${formatDate(file.updatedAt)}</p>
			</div>
			<div class="preview-surface">
				${await renderPreview(file)}
			</div>
		`;
	} catch (error) {
		viewer.innerHTML = `<div class="empty-state preview-message"><strong>Could not load file</strong><span>${escapeHtml(String(error))}</span></div>`;
		downloadLink.removeAttribute("href");
	}
};

installLinkInterceptor(document.body);

routes({
	"/": renderHome,
	"/view/:name": renderFileView,
	"/*": renderHome,
});
