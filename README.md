<div class="othello-readme">

  <style>
    .othello-readme {
      font-family: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      background: radial-gradient(130% 220% at 0% 0%, #0f172a 0, #020617 40%, #000000 100%);
      color: #e5e7eb;
      padding: 2.75rem 2.25rem;
      border-radius: 1.25rem;
      border: 1px solid rgba(148, 163, 184, 0.35);
      box-shadow:
        0 24px 80px rgba(15, 23, 42, 0.85),
        0 0 0 1px rgba(15, 23, 42, 0.6);
      margin: 1.5rem 0;
      position: relative;
      overflow: hidden;
    }

    .othello-readme::before {
      content: "";
      position: absolute;
      inset: -40%;
      background:
        radial-gradient(circle at 0% 0%, rgba(59, 130, 246, 0.16), transparent 55%),
        radial-gradient(circle at 100% 100%, rgba(45, 212, 191, 0.18), transparent 55%);
      opacity: 0.7;
      pointer-events: none;
      z-index: 0;
    }

    .othello-readme * {
      box-sizing: border-box;
    }

    .or-inner {
      position: relative;
      z-index: 1;
    }

    .or-hero {
      display: flex;
      flex-wrap: wrap;
      gap: 1.75rem;
      align-items: stretch;
      justify-content: space-between;
      margin-bottom: 2.5rem;
    }

    .or-title-block {
      flex: 1 1 260px;
      min-width: 0;
    }

    .or-title {
      display: flex;
      align-items: center;
      gap: 0.75rem;
      margin-bottom: 0.5rem;
    }

    .or-title h1 {
      font-size: 2.1rem;
      font-weight: 750;
      letter-spacing: 0.04em;
      text-transform: uppercase;
      color: #f9fafb;
      margin: 0;
      white-space: nowrap;
    }

    .or-title .or-sub {
      font-size: 0.75rem;
      text-transform: uppercase;
      letter-spacing: 0.22em;
      color: #9ca3af;
      padding: 0.15rem 0.6rem;
      border-radius: 999px;
      border: 1px solid rgba(75, 85, 99, 0.9);
      background: linear-gradient(135deg, rgba(15, 23, 42, 0.9), rgba(15, 23, 42, 0.6));
    }

    .or-tagline {
      margin: 0.4rem 0 1rem;
      color: #d1d5db;
      font-size: 0.98rem;
      max-width: 38rem;
    }

    .or-badges {
      display: flex;
      flex-wrap: wrap;
      gap: 0.5rem;
    }

    .or-badge {
      font-size: 0.72rem;
      text-transform: uppercase;
      letter-spacing: 0.13em;
      padding: 0.25rem 0.7rem;
      border-radius: 999px;
      border: 1px solid rgba(148, 163, 184, 0.6);
      background: rgba(15, 23, 42, 0.9);
      color: #e5e7eb;
      display: inline-flex;
      align-items: center;
      gap: 0.35rem;
      white-space: nowrap;
    }

    .or-badge-primary {
      border-color: rgba(96, 165, 250, 0.95);
      background: radial-gradient(circle at 0 0, rgba(59, 130, 246, 0.35), rgba(15, 23, 42, 0.9));
      color: #eff6ff;
    }

    .or-pill-dot {
      width: 0.45rem;
      height: 0.45rem;
      border-radius: 999px;
      background: radial-gradient(circle, #22c55e 0, #16a34a 40%, #065f46 100%);
      box-shadow: 0 0 10px rgba(34, 197, 94, 0.85);
    }

    .or-hero-card {
      flex: 0 1 260px;
      min-width: 0;
      background: radial-gradient(circle at 0 0, rgba(59, 130, 246, 0.35), rgba(15, 23, 42, 0.95));
      border-radius: 1rem;
      border: 1px solid rgba(148, 163, 184, 0.6);
      padding: 1rem 1.2rem;
      display: flex;
      flex-direction: column;
      justify-content: space-between;
      gap: 0.75rem;
      position: relative;
      overflow: hidden;
    }

    .or-hero-card::after {
      content: "x86_64";
      position: absolute;
      right: -1.5rem;
      bottom: -0.4rem;
      font-size: 3.5rem;
      font-weight: 800;
      opacity: 0.06;
      letter-spacing: 0.24em;
      text-transform: uppercase;
    }

    .or-hero-label {
      font-size: 0.74rem;
      text-transform: uppercase;
      letter-spacing: 0.18em;
      color: #bfdbfe;
      margin-bottom: 0.35rem;
    }

    .or-hero-list {
      list-style: none;
      padding: 0;
      margin: 0 0 0.25rem;
      font-size: 0.88rem;
      color: #e5e7eb;
    }

    .or-hero-list li {
      display: flex;
      align-items: flex-start;
      gap: 0.45rem;
      margin-bottom: 0.32rem;
    }

    .or-hero-bullet {
      width: 0.25rem;
      height: 0.25rem;
      border-radius: 999px;
      margin-top: 0.4rem;
      background: linear-gradient(135deg, #60a5fa, #22c55e);
      box-shadow: 0 0 0 1px rgba(15, 23, 42, 0.9);
    }

    .or-hero-meta {
      display: flex;
      flex-wrap: wrap;
      gap: 0.4rem;
      font-size: 0.75rem;
      color: #9ca3af;
    }

    .or-hero-meta span {
      padding: 0.15rem 0.45rem;
      border-radius: 999px;
      border: 1px solid rgba(148, 163, 184, 0.6);
      background: rgba(15, 23, 42, 0.9);
    }

    .or-section {
      margin-bottom: 2.25rem;
    }

    .or-section-header {
      display: flex;
      align-items: baseline;
      gap: 0.6rem;
      margin-bottom: 1rem;
    }

    .or-section-header h2 {
      font-size: 1.15rem;
      margin: 0;
      font-weight: 650;
      color: #e5e7eb;
    }

    .or-section-accent {
      flex: 1;
      height: 1px;
      background: linear-gradient(to right, rgba(96, 165, 250, 0.7), transparent);
      opacity: 0.75;
    }

    .or-section p {
      margin: 0.4rem 0;
      font-size: 0.93rem;
      line-height: 1.6;
      color: #e5e7eb;
    }

    .or-section p strong {
      color: #f9fafb;
    }

    .or-list {
      margin: 0.5rem 0;
      padding-left: 1.1rem;
      font-size: 0.93rem;
      color: #e5e7eb;
    }

    .or-list li {
      margin-bottom: 0.25rem;
    }

    .or-grid {
      display: grid;
      gap: 1rem;
    }

    @media (min-width: 820px) {
      .or-grid-3 {
        grid-template-columns: repeat(3, minmax(0, 1fr));
      }
      .or-grid-2 {
        grid-template-columns: repeat(2, minmax(0, 1fr));
      }
    }

    .or-card {
      border-radius: 0.9rem;
      border: 1px solid rgba(148, 163, 184, 0.45);
      background: radial-gradient(circle at 0 0, rgba(30, 64, 175, 0.45), rgba(15, 23, 42, 0.98));
      padding: 0.9rem 1rem;
      font-size: 0.9rem;
    }

    .or-card h3 {
      margin: 0 0 0.4rem;
      font-size: 0.98rem;
      font-weight: 600;
      color: #f9fafb;
    }

    .or-card p {
      margin: 0.25rem 0;
      color: #e5e7eb;
      font-size: 0.9rem;
    }

    .or-card ul {
      margin: 0.3rem 0 0;
      padding-left: 1.1rem;
      font-size: 0.88rem;
      color: #e5e7eb;
    }

    .or-pipeline {
      display: flex;
      flex-direction: column;
      gap: 0.8rem;
      font-size: 0.88rem;
    }

    .or-step {
      display: grid;
      grid-template-columns: auto 1fr;
      gap: 0.4rem 0.7rem;
      align-items: baseline;
    }

    .or-step-label {
      font-size: 0.72rem;
      text-transform: uppercase;
      letter-spacing: 0.16em;
      color: #a5b4fc;
      padding: 0.12rem 0.5rem;
      border-radius: 999px;
      border: 1px solid rgba(129, 140, 248, 0.85);
      background: rgba(17, 24, 39, 0.98);
      white-space: nowrap;
    }

    .or-step-title {
      font-weight: 550;
      color: #e5e7eb;
    }

    .or-step-body {
      grid-column: 2 / -1;
      color: #cbd5f5;
    }

    pre, code {
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
      font-size: 0.82rem;
    }

    .or-code {
      margin-top: 0.5rem;
      border-radius: 0.7rem;
      border: 1px solid rgba(15, 23, 42, 0.95);
      background: radial-gradient(circle at 0 0, rgba(15, 23, 42, 0.95), rgba(15, 23, 42, 0.98));
      padding: 0.75rem 0.9rem;
      overflow-x: auto;
      box-shadow: inset 0 0 0 1px rgba(31, 41, 55, 0.9);
    }

    .or-code pre {
      margin: 0;
      white-space: pre;
      color: #e5e7eb;
    }

    .or-inline {
      padding: 0.08rem 0.3rem;
      border-radius: 0.35rem;
      background: rgba(15, 23, 42, 0.98);
      border: 1px solid rgba(55, 65, 81, 0.9);
      color: #e5e7eb;
    }

    .or-kv {
      display: flex;
      flex-wrap: wrap;
      gap: 0.35rem 0.85rem;
      font-size: 0.85rem;
      margin-top: 0.4rem;
    }

    .or-kv span {
      padding: 0.18rem 0.5rem;
      border-radius: 999px;
      border: 1px solid rgba(75, 85, 99, 0.85);
      background: rgba(17, 24, 39, 0.98);
      color: #d1d5db;
    }

    .or-link {
      color: #bfdbfe;
      text-decoration: none;
      border-bottom: 1px solid rgba(191, 219, 254, 0.5);
      padding-bottom: 0.05rem;
    }

    .or-link:hover {
      color: #eff6ff;
      border-bottom-color: rgba(191, 219, 254, 0.9);
    }

    .or-mono {
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
    }

    .or-muted {
      color: #9ca3af;
      font-size: 0.84rem;
    }

    .or-roadmap {
      display: grid;
      gap: 0.75rem;
      font-size: 0.9rem;
    }

    .or-chip-row {
      display: flex;
      flex-wrap: wrap;
      gap: 0.35rem;
      margin-top: 0.4rem;
    }

    .or-chip {
      font-size: 0.78rem;
      padding: 0.18rem 0.55rem;
      border-radius: 999px;
      border: 1px solid rgba(148, 163, 184, 0.7);
      background: rgba(15, 23, 42, 0.98);
      color: #e5e7eb;
    }

    .or-footer-note {
      margin-top: 0.75rem;
      font-size: 0.82rem;
      color: #9ca3af;
    }

    @media (max-width: 640px) {
      .othello-readme {
        padding: 1.75rem 1.3rem;
      }

      .or-title h1 {
        font-size: 1.6rem;
        white-space: normal;
      }

      .or-hero-card::after {
        display: none;
      }
    }
  </style>

  <div class="or-inner">

    <!-- HERO -->
    <section class="or-hero">
      <div class="or-title-block">
        <div class="or-title">
          <h1>Othello OS</h1>
          <span class="or-sub">Bare-Metal Research OS</span>
        </div>
        <p class="or-tagline">
          A minimal, hand-crafted operating system for x86_64 that walks through the entire CPU boot
          process—from 16-bit real mode, through protected mode, into 64-bit long mode—backed by a Rust/C kernel
          with both CLI and GUI capabilities.
        </p>
        <div class="or-badges">
          <span class="or-badge or-badge-primary">
            <span class="or-pill-dot"></span>
            Version&nbsp;1.0
          </span>
          <span class="or-badge">x86_64</span>
          <span class="or-badge">BIOS&nbsp;|&nbsp;MBR + Stage-2</span>
          <span class="or-badge">Rust Kernel</span>
          <span class="or-badge">Assembly First</span>
        </div>
      </div>

      <aside class="or-hero-card">
        <div>
          <div class="or-hero-label">Project Snapshot</div>
          <ul class="or-hero-list">
            <li>
              <span class="or-hero-bullet"></span>
              <span>Shows every step of the boot pipeline with explicit transitions between CPU modes.</span>
            </li>
            <li>
              <span class="or-hero-bullet"></span>
              <span>Modular subsystems: GDT/IDT, paging, disk loader, kernel hand-off.</span>
            </li>
            <li>
              <span class="or-hero-bullet"></span>
              <span>Acts as a playground for OS and micro-architecture experimentation.</span>
            </li>
          </ul>
        </div>
        <div class="or-hero-meta">
          <span>Research / Educational</span>
          <span>Not for production use</span>
        </div>
      </aside>
    </section>

    <!-- OVERVIEW -->
    <section class="or-section" id="overview">
      <div class="or-section-header">
        <h2>Overview</h2>
        <div class="or-section-accent"></div>
      </div>

      <p>
        <strong>Othello</strong> is a stripped-down operating system focused on being readable, hackable, and
        academically useful rather than feature-complete. It is implemented as:
      </p>

      <ul class="or-list">
        <li><strong>Bootloader:</strong> 16-bit and 32-bit x86_64 Assembly (MBR + stage-2 loader)</li>
        <li><strong>Kernel:</strong> primarily Rust, with C and Assembly where bare-metal control is needed</li>
        <li><strong>User-facing layer:</strong> simple CLI, with groundwork for a basic GUI on top of the framebuffer</li>
      </ul>

      <p>
        The design emphasizes:
      </p>

      <ul class="or-list">
        <li>
          <strong>Education:</strong> Expose each step of mode transitions and low-level hardware setup rather than
          hide it behind frameworks.
        </li>
        <li>
          <strong>Modularity:</strong> Keep subsystems
          <span class="or-inline">GDT</span>,
          <span class="or-inline">IDT</span>,
          <span class="or-inline">disk loader</span>,
          <span class="or-inline">mode switcher</span>,
          <span class="or-inline">print / debug routines</span>,
          <span class="or-inline">kernel entry</span>
          separated and well-documented.
        </li>
        <li>
          <strong>Simplicity:</strong> Flat memory model + identity-mapped paging initially, to reduce cognitive
          overhead while you learn.
        </li>
        <li>
          <strong>Extensibility:</strong> Clear hand-off points such as
          <span class="or-inline">BEGIN_32BIT</span>,
          <span class="or-inline">BEGIN_64BIT</span>,
          <span class="or-inline">kernel_main</span>
          so you can drop in your own experiments.
        </li>
      </ul>
    </section>

    <!-- ARCHITECTURE -->
    <section class="or-section" id="architecture">
      <div class="or-section-header">
        <h2>Boot &amp; Architecture</h2>
        <div class="or-section-accent"></div>
      </div>

      <div class="or-grid or-grid-2">
        <div class="or-card">
          <h3>Boot Pipeline</h3>
          <div class="or-pipeline">
            <div class="or-step">
              <span class="or-step-label">Stage&nbsp;0</span>
              <div class="or-step-title">BIOS &rarr; MBR (Real Mode)</div>
              <div class="or-step-body">
                BIOS loads the 512-byte MBR at <span class="or-inline">0x7C00</span>, enabling the first tiny boot
                stub. This code sets up a basic stack, verifies disk geometry, and locates the stage-2 loader.
              </div>
            </div>
            <div class="or-step">
              <span class="or-step-label">Stage&nbsp;1</span>
              <div class="or-step-title">Stage-2 Loader (16-bit &rarr; 32-bit)</div>
              <div class="or-step-body">
                The loader pulls additional sectors from disk using BIOS
                <span class="or-inline">INT&nbsp;13h</span>, builds and loads a
                <span class="or-inline">GDT</span>, and performs the real-mode &rarr; protected-mode transition.
              </div>
            </div>
            <div class="or-step">
              <span class="or-step-label">Stage&nbsp;2</span>
              <div class="or-step-title">Protected Mode Setup</div>
              <div class="or-step-body">
                Basic paging is configured (identity-mapped), segment descriptors are finalized, and low-level
                print/debug routines are available for tracing the bring-up sequence.
              </div>
            </div>
            <div class="or-step">
              <span class="or-step-label">Stage&nbsp;3</span>
              <div class="or-step-title">Long Mode &amp; Kernel Entry</div>
              <div class="or-step-body">
                The CPU switches into 64-bit long mode, the Rust kernel image is mapped, and execution jumps into
                <span class="or-inline">kernel_main()</span>, where higher-level services (CLI, scheduler, future GUI)
                come online.
              </div>
            </div>
          </div>
        </div>

        <div class="or-card">
          <h3>Core Subsystems</h3>
          <ul>
            <li>
              <strong>Memory &amp; Paging:</strong> flat model + identity mapping to keep early debugging simple,
              with paging infrastructure ready for more advanced layouts.
            </li>
            <li>
              <strong>Interrupts &amp; Exceptions:</strong> groundwork for IDT entries, fault reporting, and a clean
              hand-off into Rust handlers.
            </li>
            <li>
              <strong>Framebuffer / Output:</strong> early text/graphics routines for boot logging, shell output,
              and GUI experiments.
            </li>
            <li>
              <strong>Kernel Services:</strong> initialization framework in Rust (heap, basic runtime, panic hooks)
              designed for extension into a full RTOS or general-purpose OS.
            </li>
          </ul>

          <div class="or-kv">
            <span>Architecture: x86_64</span>
            <span>Boot: BIOS / MBR</span>
            <span>Languages: Rust, C, ASM</span>
          </div>
        </div>
      </div>
    </section>

    <!-- FEATURES -->
    <section class="or-section" id="features">
      <div class="or-section-header">
        <h2>Features &amp; Design Goals</h2>
        <div class="or-section-accent"></div>
      </div>

      <div class="or-grid or-grid-3">
        <div class="or-card">
          <h3>Educational OS</h3>
          <p>
            Othello aims to be a guided tour of modern x86_64 boot and kernel bring-up, not a large production kernel.
          </p>
          <ul>
            <li>Traceable boot logs with clear “what just happened?” comments.</li>
            <li>Intentional, minimal abstractions over hardware.</li>
            <li>Great starting point for OS courses or self-study.</li>
          </ul>
        </div>

        <div class="or-card">
          <h3>Modular Layout</h3>
          <p>
            Each major responsibility lives in its own unit so you can read, replace, or extend it independently.
          </p>
          <ul>
            <li>Boot stages, GDT/IDT, and paging defined in isolated modules.</li>
            <li>Rust kernel with well-defined entry points.</li>
            <li>Easy to plug in your own scheduler, allocator, or drivers.</li>
          </ul>
        </div>

        <div class="or-card">
          <h3>CLI &amp; GUI Foundations</h3>
          <p>
            A basic command-line interface exists for early debugging, with hooks for an experimental GUI layered on
            the framebuffer.
          </p>
          <ul>
            <li>Text-mode shell for interacting with the running kernel.</li>
            <li>Framebuffer primitives for drawing pixels and UI widgets.</li>
            <li>Natural path toward a lightweight desktop for demos.</li>
          </ul>
        </div>
      </div>
    </section>

    <!-- GETTING STARTED -->
    <section class="or-section" id="getting-started">
      <div class="or-section-header">
        <h2>Getting Started</h2>
        <div class="or-section-accent"></div>
      </div>

      <p>
        The full, step-by-step build pipeline—assembling the bootloader, compiling the Rust/C kernel, and wrapping
        everything into an El Torito bootable ISO—is described in
        <a class="or-link" href="OS_Build/BUILDING.md"><span class="or-mono">OS_Build/BUILDING.md</span></a>.
        Below is a quick-start summary.
      </p>

      <div class="or-grid or-grid-2">
        <div>
          <h3 style="font-size:0.96rem;margin-bottom:0.4rem;">Prerequisites</h3>
          <ul class="or-list">
            <li><strong>Rust</strong> (with <span class="or-inline">x86_64-unknown-none</span> or similar bare-metal target)</li>
            <li><strong>Assembler</strong> (e.g., <span class="or-inline">nasm</span> or <span class="or-inline">yasm</span>)</li>
            <li><strong>C toolchain</strong> (e.g., <span class="or-inline">clang</span> or <span class="or-inline">gcc</span>)</li>
            <li><strong>QEMU</strong> or another x86_64 hypervisor/emulator</li>
            <li>Make / PowerShell (depending on your host environment)</li>
          </ul>
        </div>

        <div>
          <h3 style="font-size:0.96rem;margin-bottom:0.4rem;">Clone &amp; Build (Example)</h3>
          <div class="or-code">
            <pre><code># Clone the repository
git clone https://github.com/&lt;your-user&gt;/Othello-OS.git
cd Othello-OS

# Navigate into the build orchestration directory
cd OS_Build

# Build disk image + run in QEMU (scripts vary by host OS)
./build-and-run.sh        # Linux/macOS
# or
./build-and-run.ps1       # Windows PowerShell</code></pre>
          </div>
          <p class="or-muted">
            Exact commands, environment variables, and toolchain versions are documented in
            <span class="or-mono">OS_Build/BUILDING.md</span>.
          </p>
        </div>
      </div>
    </section>

    <!-- REPO LAYOUT -->
    <section class="or-section" id="layout">
      <div class="or-section-header">
        <h2>Repository Layout</h2>
        <div class="or-section-accent"></div>
      </div>

      <div class="or-grid or-grid-2">
        <div class="or-card">
          <h3>High-Level Structure</h3>
          <div class="or-code">
            <pre><code>Othello-OS/
├── OS_Build/          # Build scripts, linker scripts, bootloader &amp; ISO pipeline
│   ├── BUILDING.md    # Detailed build and tooling documentation
│   └── ...            # Stage-1/2 bootloader sources, disk image logic
├── Rust-Kernel/       # Rust kernel crate (kernel_main, init, drivers)
│   └── ...            
├── docs/              # Optional design notes / diagrams (if present)
└── README.md          # This document</code></pre>
          </div>
          <p class="or-footer-note">
            Folder names may evolve as the project grows; check the tree above and inline comments for the latest
            layout.
          </p>
        </div>

        <div class="or-card">
          <h3>Entry Points Worth Hacking</h3>
          <ul>
            <li>
              <span class="or-inline">BEGIN_32BIT</span>,
              <span class="or-inline">BEGIN_64BIT</span> – assembly labels for mode transitions.
            </li>
            <li>
              <span class="or-inline">kernel_main()</span> – central Rust entry point after the CPU is fully set up.
            </li>
            <li>
              <span class="or-inline">print_* / debug_*()</span> – early boot logging utilities.
            </li>
            <li>
              <span class="or-inline">init_paging()</span>,
              <span class="or-inline">init_gdt()</span>,
              <span class="or-inline">init_idt()</span> – good places to explore memory and interrupt design.
            </li>
          </ul>
        </div>
      </div>
    </section>

    <!-- ROADMAP -->
    <section class="or-section" id="roadmap">
      <div class="or-section-header">
        <h2>Roadmap</h2>
        <div class="or-section-accent"></div>
      </div>

      <div class="or-roadmap">
        <div>
          <strong>Short-Term</strong>
          <ul class="or-list">
            <li>Refine boot diagnostics and error codes for each stage.</li>
            <li>Stabilize CLI commands for inspecting memory, paging, and CPU state.</li>
            <li>Harden mode-switch logic and interrupt handling for edge cases.</li>
          </ul>
        </div>
        <div>
          <strong>Medium-Term</strong>
          <ul class="or-list">
            <li>Add a minimal scheduler and task abstraction for concurrent workloads.</li>
            <li>Introduce a basic filesystem driver for loading user programs.</li>
            <li>Expand GUI layer with simple windows/widgets for demos.</li>
          </ul>
        </div>
        <div>
          <strong>Long-Term / Experimental</strong>
          <ul class="or-list">
            <li>Evaluate RTOS-style scheduling for soft real-time workloads.</li>
            <li>Explore SMP (multi-core bring-up) and inter-CPU communication.</li>
            <li>Integrate more advanced memory management (higher-half kernel, user space, isolation).</li>
          </ul>
        </div>
      </div>

      <div class="or-chip-row">
        <span class="or-chip">OS Dev</span>
        <span class="or-chip">Bare-Metal</span>
        <span class="or-chip">Rust</span>
        <span class="or-chip">x86_64</span>
        <span class="or-chip">Bootloader</span>
      </div>
    </section>

    <!-- CONTRIBUTING / LICENSE -->
    <section class="or-section" id="contributing">
      <div class="or-section-header">
        <h2>Contributing &amp; License</h2>
        <div class="or-section-accent"></div>
      </div>

      <p>
        Othello is primarily a research and learning project, but high-quality issues, design notes, and small
        pull requests that improve clarity, tooling, or documentation are welcome.
      </p>

      <ul class="or-list">
        <li>Keep changes small and focused (one subsystem or fix at a time).</li>
        <li>Prefer comments that explain <em>why</em> a low-level sequence exists, not just what it does.</li>
        <li>Avoid large feature drops without discussion; this project values readability first.</li>
      </ul>

      <p>
        Licensing details will appear here once finalized. Until then, treat the project as read-only educational
        material unless the repository specifies otherwise.
      </p>
    </section>

  </div>
</div>
