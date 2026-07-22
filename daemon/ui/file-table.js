const COLUMNS = [
  { key: "icon", label: "", width: 28, min: 28 },
  { key: "name", label: "Name", width: 360, min: 160, sort: "name" },
  { key: "online", label: "Online", width: 90, min: 75 },
  { key: "type", label: "Type", width: 120, min: 90 },
  { key: "size", label: "Size", width: 90, min: 70, sort: "sizeValue" },
  { key: "hash", label: "Hash", width: 260, min: 160 },
  { key: "replicas", label: "Replicas", width: 100, min: 80, sort: "replicaCount" },
  { key: "modified", label: "Modified", width: 120, min: 90, sort: "modifiedValue" },
];

export default class FileTable {
  constructor(element, ctx) {
    this.element = element;
    this.ctx = ctx;
    this.widths = COLUMNS.map((column) => column.width);
    this.sortKey = null;
    this.sortDescending = false;
  }

  mount(props) { this.setProps(props); }

  setProps(props) {
    const serverSort = Boolean(props.serverSort);
    if (serverSort) {
      this.sortKey = String(props.sortKey || "name");
      this.sortDescending = Boolean(props.sortDescending);
    }
    const grid = () => this.widths.map((width, index) => `${Math.max(width, COLUMNS[index].min)}px`).join(" ");
    const root = document.createElement("div");
    root.style.width = "100%";
    root.style.minWidth = "0";
    root.style.height = "100%";
    root.style.overflow = "auto";
    root.style.background = "#fff";
    const render = () => {
      root.replaceChildren();
      const header = document.createElement("div");
      header.style.display = "grid";
      header.style.gridTemplateColumns = grid();
      header.style.minWidth = "max-content";
      header.style.background = "#f8fafb";
      header.style.color = "#6b7280";
      COLUMNS.forEach((column, index) => {
        const cell = document.createElement("div");
        cell.textContent = column.label + (this.sortKey === column.sort ? (this.sortDescending ? " ↓" : " ↑") : "");
        cell.style.position = "relative";
        cell.style.padding = "5px 8px";
        cell.style.boxSizing = "border-box";
        if (column.sort) {
          cell.style.cursor = "pointer";
          cell.onclick = () => {
            if (serverSort) {
              this.ctx.emit("sort", { key: column.sort });
              return;
            }
            if (this.sortKey === column.sort) this.sortDescending = !this.sortDescending;
            else { this.sortKey = column.sort; this.sortDescending = false; }
            render();
          };
        }
        if (index < COLUMNS.length - 1) {
          const handle = document.createElement("div");
          handle.style.position = "absolute";
          handle.style.top = "0";
          handle.style.right = "0";
          handle.style.width = "10px";
          handle.style.height = "100%";
          handle.style.cursor = "col-resize";
          handle.style.borderRight = "1px solid #cbd5e1";
          handle.style.zIndex = "2";
          handle.style.touchAction = "none";
          handle.onpointerdown = (event) => {
            event.preventDefault();
            event.stopPropagation();
            handle.setPointerCapture?.(event.pointerId);
            const startX = event.clientX;
            const startWidth = this.widths[index];
            const move = (moveEvent) => {
              this.widths[index] = Math.max(COLUMNS[index].min, startWidth + moveEvent.clientX - startX);
              render();
            };
            window.addEventListener("pointermove", move);
            window.addEventListener("pointerup", () => {
              window.removeEventListener("pointermove", move);
              handle.releasePointerCapture?.(event.pointerId);
            }, { once: true });
          };
          cell.append(handle);
        }
        header.append(cell);
      });
      root.append(header);
      const rows = [...(props.rows || [])];
      if (this.sortKey && !serverSort) {
        rows.sort((left, right) => {
          const leftValue = left[this.sortKey];
          const rightValue = right[this.sortKey];
          const result = typeof leftValue === "string"
            ? leftValue.localeCompare(rightValue)
            : Number(leftValue) - Number(rightValue);
          return this.sortDescending ? -result : result;
        });
      }
      for (const row of rows) {
        const line = document.createElement("button");
        line.type = "button";
        line.style.display = "grid";
        line.style.gridTemplateColumns = grid();
        line.style.minWidth = "max-content";
        line.style.width = "100%";
        line.style.padding = "0";
        line.style.border = "0";
        line.style.borderBottom = "1px solid #edf1f2";
        line.style.background = "#fff";
        line.style.textAlign = "left";
        line.style.cursor = "pointer";
        line.onclick = () => this.ctx.emit("open", { index: Number(row.index) });
        for (const column of COLUMNS) {
          const cell = document.createElement("div");
          if (column.key === "online") {
            const online = Boolean(row.online);
            cell.textContent = online ? "● Online" : "○ Offline";
            cell.style.color = online ? "#15803d" : "#b42318";
            cell.title = String(row.sourceTooltip ?? "");
          } else {
            cell.textContent = String(row[column.key] ?? "");
            cell.title = cell.textContent;
          }
          cell.style.padding = "7px 8px";
          cell.style.boxSizing = "border-box";
          cell.style.overflowWrap = "anywhere";
          cell.style.color = column.key === "name" ? "#0f6175" : "#111827";
          line.append(cell);
        }
        root.append(line);
      }
    };
    render();
    this.element.replaceChildren(root);
  }

  dispose() { this.element.replaceChildren(); }
}
