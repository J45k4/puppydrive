export default class FolderRow {
  constructor(element, ctx) {
    this.element = element;
    this.ctx = ctx;
  }

  mount(props) {
    this.setProps(props);
  }

  setProps(props) {
    const background = props.selected ? "#e5f4f7" : "#ffffff";
    const emit = (name, payload = {}) =>
      this.ctx.emit(name, { ...payload, index: Number(props.index) });
    const showContextMenu = (event) => {
      event.preventDefault();
      emit("context", { x: event.clientX, y: event.clientY });
    };

    const root = document.createElement("div");
    root.style.display = "flex";
    root.style.width = "100%";
    root.style.minWidth = "0";
    root.style.paddingLeft = `${Number(props.indent) || 0}px`;
    root.style.background = background;
    root.oncontextmenu = showContextMenu;

    const arrow = document.createElement("button");
    arrow.textContent = props.expanded ? "⌄" : "›";
    arrow.type = "button";
    arrow.style.width = "22px";
    arrow.style.minWidth = "22px";
    arrow.style.padding = "2px";
    arrow.style.border = "1px solid transparent";
    arrow.style.background = background;
    arrow.style.color = "#687385";
    arrow.style.cursor = "pointer";
    arrow.onclick = () => emit("toggle");
    arrow.oncontextmenu = showContextMenu;

    const label = document.createElement("button");
    label.textContent = String(props.label);
    label.type = "button";
    label.style.flex = "1";
    label.style.minWidth = "0";
    label.style.padding = "2px";
    label.style.border = "1px solid transparent";
    label.style.background = background;
    label.style.color = "#374151";
    label.style.textAlign = "left";
    label.style.cursor = "pointer";
    label.onclick = () => emit("open");
    label.ondblclick = () => emit("navigate");
    label.oncontextmenu = showContextMenu;

    root.append(arrow, label);
    this.element.replaceChildren(root);
  }

  dispose() {
    this.element.replaceChildren();
  }
}
