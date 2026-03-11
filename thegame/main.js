import init, { } from "./thegame.js";

function resizeCanvas() {
  const canvas = document.getElementById("canvas");
  console.log(canvas.clientWidth);
  canvas.width = canvas.clientWidth;
  canvas.height = canvas.clientHeight;
}

function setupFullscreen() {
  const btn = document.getElementById("fullscreen-btn");
  const wrapper = document.getElementById("canvas-wrapper");

  btn.addEventListener("click", () => {
    if (!document.fullscreenElement) {
      wrapper.requestFullscreen();
    } else {
      document.exitFullscreen();
    }
  });

  // Update button icon based on fullscreen state
  document.addEventListener("fullscreenchange", () => {
    btn.textContent = document.fullscreenElement ? "✕" : "⛶";
  });

  // Also allow Escape to exit (browsers do this natively,
  // but this keeps our icon in sync)
  document.addEventListener("keydown", (e) => {
    if (e.key === "f" || e.key === "F") {
      btn.click();
    }
  });
}

async function main() {
  await init();

  if (!navigator.gpu) {
    alert("WebGPU not supported!");
    return;
  }

  // resizeCanvas();
  setupFullscreen();
  // run_web();
}

main().catch(console.error);