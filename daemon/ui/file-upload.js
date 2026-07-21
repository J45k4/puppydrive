export default class FileUpload {
  constructor(element, ctx) {
    this.element = element;
    this.ctx = ctx;
    this.props = {};
  }

  mount(props) {
    this.setProps(props);
  }

  setProps(props) {
    this.props = props || {};
    this.render();
  }

  render() {
    const root = document.createElement("div");
    root.style.display = "grid";
    root.style.gap = "14px";
    root.style.maxWidth = "680px";
    root.style.padding = "18px";
    root.style.border = "1px solid #dce5e8";
    root.style.borderRadius = "6px";
    root.style.background = "#ffffff";

    const inboxes = Array.isArray(this.props.inboxes) ? this.props.inboxes : [];
    const inboxLabel = document.createElement("label");
    inboxLabel.textContent = "1. Choose Inbox";
    inboxLabel.style.display = "grid";
    inboxLabel.style.gap = "6px";
    inboxLabel.style.color = "#374151";
    inboxLabel.style.fontWeight = "600";
    const inbox = document.createElement("select");
    inbox.style.boxSizing = "border-box";
    inbox.style.width = "100%";
    inbox.style.padding = "8px";
    inbox.style.border = "1px solid #cbd5e1";
    inbox.style.borderRadius = "4px";
    inbox.style.background = "#ffffff";
    const placeholder = document.createElement("option");
    placeholder.value = "";
    placeholder.textContent = inboxes.length ? "Choose an Inbox" : "No Inboxes configured";
    inbox.append(placeholder);
    for (const configuredInbox of inboxes) {
      const choice = document.createElement("option");
      choice.value = String(configuredInbox.id || "");
      choice.textContent = `${String(configuredInbox.name || "Inbox")} — ${String(configuredInbox.folder || "")}`;
      inbox.append(choice);
    }
    inbox.disabled = !inboxes.length;
    inboxLabel.append(inbox);

    const filesLabel = document.createElement("label");
    filesLabel.textContent = "2. Choose files";
    filesLabel.style.display = "grid";
    filesLabel.style.gap = "6px";
    filesLabel.style.color = "#374151";
    filesLabel.style.fontWeight = "600";
    const fileInput = document.createElement("input");
    fileInput.type = "file";
    fileInput.multiple = true;
    fileInput.style.fontWeight = "400";
    filesLabel.append(fileInput);

    const location = document.createElement("div");
    location.textContent = `Inboxes are configured in Settings under: ${String(this.props.root || "/")}`;
    location.style.color = "#6b7280";
    location.style.fontSize = "13px";
    location.style.overflowWrap = "anywhere";

    const upload = document.createElement("button");
    upload.type = "button";
    upload.textContent = "Upload selected files";
    upload.style.justifySelf = "start";
    upload.style.padding = "8px 12px";
    upload.style.border = "1px solid #0f6175";
    upload.style.borderRadius = "4px";
    upload.style.background = "#0f6175";
    upload.style.color = "#ffffff";
    upload.style.cursor = "pointer";

    const status = document.createElement("div");
    status.setAttribute("role", "status");
    status.style.color = "#4b5563";
    status.style.whiteSpace = "pre-line";

    upload.onclick = async () => {
      if (!inbox.value) {
        status.textContent = "Choose an Inbox first.";
        status.style.color = "#b42318";
        return;
      }
      const files = Array.from(fileInput.files || []);
      if (!files.length) {
        status.textContent = "Choose at least one file first.";
        status.style.color = "#b42318";
        return;
      }

      upload.disabled = true;
      fileInput.disabled = true;
      inbox.disabled = true;
      const uploaded = [];
      const failed = [];
      for (const [index, file] of files.entries()) {
        status.textContent = `Uploading ${index + 1} of ${files.length}: ${file.name}`;
        status.style.color = "#4b5563";
        try {
          const response = await fetch(`/uploads?inbox=${encodeURIComponent(inbox.value)}`, {
            method: "POST",
            headers: {
              "x-puppydrive-filename": encodeURIComponent(file.name),
              "content-type": file.type || "application/octet-stream",
            },
            body: file,
          });
          const message = (await response.text()).trim();
          if (response.ok) uploaded.push(file.name);
          else failed.push(`${file.name}: ${message || `upload failed (${response.status})`}`);
        } catch (error) {
          failed.push(`${file.name}: ${error instanceof Error ? error.message : "network error"}`);
        }
      }

      upload.disabled = false;
      fileInput.disabled = false;
      inbox.disabled = false;
      const lines = [];
      if (uploaded.length) lines.push(`${uploaded.length} file${uploaded.length === 1 ? "" : "s"} uploaded.`);
      if (failed.length) lines.push(...failed);
      status.textContent = lines.join("\n");
      status.style.color = failed.length ? "#b42318" : "#16803a";
      if (uploaded.length) {
        fileInput.value = "";
        this.ctx.emit("uploaded", { count: uploaded.length });
      }
    };

    root.append(inboxLabel, filesLabel, location, upload, status);
    this.element.replaceChildren(root);
  }

  dispose() {
    this.element.replaceChildren();
  }
}
