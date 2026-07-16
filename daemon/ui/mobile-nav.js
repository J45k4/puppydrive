export default class MobileNav {
  constructor(element) {
    this.element = element;
    this.mobileQuery = window.matchMedia("(max-width: 720px)");
    this.close = () => {
      delete document.body.dataset.mobileNavOpen;
    };
    this.handleDocumentClick = (event) => {
      const target = event.target;
      if (!(target instanceof Element)) return;

      if (
        target.closest('[name~="mobile-nav-close"]') ||
        target.closest('[name~="mobile-nav-scrim"]')
      ) {
        this.close();
        return;
      }

      const sidebar = target.closest('[name~="app-sidebar"]');
      if (sidebar && (target.closest("a") || target.closest("button"))) {
        this.close();
      }
    };
    this.handleKeyDown = (event) => {
      if (event.key === "Escape") this.close();
    };
    this.handleViewportChange = (event) => {
      if (!event.matches) this.close();
    };
  }

  mount() {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = "☰";
    button.title = "Open navigation";
    button.setAttribute("aria-label", "Open navigation");
    button.style.width = "44px";
    button.style.height = "38px";
    button.style.padding = "4px";
    button.style.border = "1px solid #dce5e8";
    button.style.borderRadius = "4px";
    button.style.background = "#ffffff";
    button.style.color = "#0f6175";
    button.style.fontSize = "20px";
    button.style.cursor = "pointer";
    button.onclick = () => {
      document.body.dataset.mobileNavOpen = "true";
    };

    this.element.replaceChildren(button);
    document.addEventListener("click", this.handleDocumentClick);
    document.addEventListener("keydown", this.handleKeyDown);
    this.mobileQuery.addEventListener("change", this.handleViewportChange);
  }

  setProps() {}

  dispose() {
    this.close();
    document.removeEventListener("click", this.handleDocumentClick);
    document.removeEventListener("keydown", this.handleKeyDown);
    this.mobileQuery.removeEventListener("change", this.handleViewportChange);
    this.element.replaceChildren();
  }
}
