const activeVideos = new Set();

function stopVideo(video) {
  video.pause();
  video.removeAttribute("src");
  video.load();
}

export default class VideoViewer {
  constructor(element) {
    this.element = element;
    this.video = null;
  }

  mount(props) {
    this.setProps(props);
  }

  setProps(props) {
    for (const activeVideo of activeVideos) stopVideo(activeVideo);
    activeVideos.clear();
    this.video = null;
    const video = document.createElement("video");
    video.src = String(props.src);
    video.controls = true;
    video.autoplay = true;
    video.preload = "metadata";
    video.style.width = "100%";
    video.style.height = "100%";
    video.style.display = "block";
    video.style.background = "#111827";
    video.style.objectFit = "contain";
    this.video = video;
    activeVideos.add(video);
    this.element.replaceChildren(video);
  }

  dispose() {
    this.stop();
    this.element.replaceChildren();
  }

  stop() {
    if (!this.video) return;
    stopVideo(this.video);
    activeVideos.delete(this.video);
    this.video = null;
  }
}
