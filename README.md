<p align="center">
  <img src="Othello.png" alt="Othello OS icon" width="96" height="96">
</p>

<h1 align="center">Othello OS</h1>

<p align="center">
  <em>Bare-metal x86_64 playground for bootloaders, kernels, desktops, and RTOS-style experiments.</em>
</p>

<p align="center">
  <sub>
    From power-on ➜ BIOS/UEFI loader ➜ long mode ➜ Rust <code>_start(boot_info)</code>.
    Minimal, modular, and unapologetically low-level.
  </sub>
</p>

<p align="center">
  <img alt="arch" src="https://img.shields.io/badge/arch-x86__64-informational?style=for-the-badge">
  <img alt="langs" src="https://img.shields.io/badge/languages-Assembly%20%7C%20Rust%20%7C%20C-orange?style=for-the-badge">
  <img alt="status" src="https://img.shields.io/badge/status-research%20/%20edu-blueviolet?style=for-the-badge">
  <img alt="version" src="https://img.shields.io/badge/version-1.2.1-success?style=for-the-badge">
</p>

<p align="center">
  <kbd>Assembly-first boot</kbd>
  <kbd>Rust kernel</kbd>
  <kbd>BIOS (MBR → Stage-2)</kbd>
  <kbd>UEFI loader</kbd>
  <kbd>Real-mode → Long-mode</kbd>
</p>

<p align="center">
  <a href="#overview">Overview</a> ·
  <a href="#boot-pipeline">Boot Pipeline</a> ·
  <a href="#features--design-goals">Features</a> ·
  <a href="#desktop--apps">Desktop &amp; Apps</a> ·
  <a href="#shell--commands">Shell &amp; Commands</a> ·
  <a href="#networking--web">Networking &amp; Web</a> ·
  <a href="#security">Security</a> ·
  <a href="#filesystem--persistence">Filesystem</a> ·
  <a href="#web-browser">Web Browser</a> ·
  <a href="#kernel-architecture">Kernel Architecture</a> ·
  <a href="#memory-model">Memory Model</a> ·
  <a href="#repo-layout">Repo Layout</a> ·
  <a href="#getting-started">Getting Started</a> ·
  <a href="#hacking-points">Hacking Points</a> ·
  <a href="#roadmap--status">Roadmap</a> ·
  <a href="#contributing">Contributing</a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/bootloader-%20hand--crafted-informational?style=flat-square">
  <img src="https://img.shields.io/badge/kernel-🦀%20rust--powered-informational?style=flat-square">
  <img src="https://img.shields.io/badge/usecase-%20osdev%20lab-informational?style=flat-square">
</p>

<div align="center">
<pre>
[ BOOT PIPELINES ]
BIOS: MBR → Stage-2 Loader → Protected Mode → Long Mode → Rust _start(boot_info)
UEFI: EFI Loader           → (already 64-bit)          → Rust _start(boot_info)
</pre>
</div>

<hr />

<h2 id="overview">Overview</h2>

<blockquote>
  <p>
    <strong>Othello</strong> is a minimal, hand-crafted operating system written in <strong>x86_64 Assembly</strong>, <strong>Rust</strong>, and <strong>C</strong>.<br />
    It demonstrates the complete CPU boot process—from firmware through <strong>protected mode</strong> and into <strong>long mode</strong>—
    while providing a real framebuffer desktop, an interactive shell, storage, and networking.
  </p>
</blockquote>

<p>Othello is designed to be:</p>
<ul>
  <li><strong>Educational</strong> – Every stage of the boot process is explicit and inspectable.</li>
  <li><strong>Modular</strong> – Core subsystems are cleanly separated (mode switching, interrupts, paging, FS, net, UI).</li>
  <li><strong>Simple</strong> – Starts with a flat memory model and identity-mapped paging.</li>
  <li><strong>Extensible</strong> – Obvious hook points for experimentation and expansion.</li>
</ul>

<br />

<div align="center">
  <table>
    <thead>
      <tr>
        <th align="center">Project Focus</th>
        <th align="center">Ideal For</th>
      </tr>
    </thead>
    <tbody>
      <tr>
        <td valign="top">
          <ul>
            <li>Understanding <strong>BIOS/UEFI → long mode</strong> in a real codebase.</li>
            <li>Seeing how <strong>GDT / IDT / paging</strong> glue together.</li>
            <li>Experimenting with <strong>Rust on bare metal</strong>.</li>
            <li>Building a tiny <strong>desktop + networking</strong> stack you can reason about end-to-end.</li>
          </ul>
        </td>
        <td valign="top">
          <ul>
            <li>Students learning <strong>OS dev</strong> / computer architecture.</li>
            <li>Hobbyists building custom kernels or RTOS experiments.</li>
            <li>Anyone who wants to step through <strong>every boot stage</strong> on x86_64.</li>
          </ul>
        </td>
      </tr>
    </tbody>
  </table>
</div>

<br />

<h3>Why this OS exists</h3>
<p>
  There are plenty of full-featured kernels; <strong>Othello is intentionally not one of them</strong>.
  Instead, it aims to be a <strong>signal-boosted lab bench</strong> for:
</p>
<ul>
  <li>Following the <strong>exact</strong> path from firmware to 64-bit Rust.</li>
  <li>Understanding how <strong>GDT / IDT / paging / mode switches</strong> actually work.</li>
  <li>Experimenting with desktop UI, networking, storage, and RTOS-style design on a small, readable codebase.</li>
</ul>

<h3>Quick facts</h3>
<table>
  <thead>
    <tr>
      <th align="left">Category</th>
      <th align="left">Details</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td>Architecture</td>
      <td>x86_64 (BIOS and UEFI boot options)</td>
    </tr>
    <tr>
      <td>Boot pipeline</td>
      <td>BIOS: MBR → Stage-2 → PM → LM · UEFI: EFI loader → LM</td>
    </tr>
    <tr>
      <td>Languages</td>
      <td>Assembly (boot), Rust (kernel/apps), C (support code)</td>
    </tr>
    <tr>
      <td>Kernel entry</td>
      <td><code>_start(boot_info)</code> (Rust)</td>
    </tr>
    <tr>
      <td>Status</td>
      <td>Early research / hobby OS (not production-ready)</td>
    </tr>
  </tbody>
</table>

<hr />

<h2 id="boot-pipeline">Boot Pipeline</h2>

<p>
  Othello is built around the <strong>full mode-transition story</strong>.
  You can trace what the CPU is doing from firmware to Rust.
</p>

<div align="center">
<pre>
[ CPU BOOT STORY ]
BIOS path: MBR @ 0x7C00 (16-bit) → Stage-2 loader → 32-bit protected mode → 64-bit long mode → Rust _start()
UEFI path: EFI loader (64-bit)   → (load kernel + framebuffer info)       → Rust _start()
</pre>
</div>

<p align="center">
  <sub><em>Both boot paths converge by entering long mode and jumping into the Rust kernel.</em></sub>
</p>

<br />

<h3>Stage-by-stage (BIOS path)</h3>

<details>
  <summary><strong>Stage 0 – MBR (16-bit real mode)</strong></summary>
  <ul>
    <li>BIOS loads the 512-byte MBR at <code>0x7C00</code>.</li>
    <li>A tiny stub sets up a stack and disk access basics.</li>
    <li>Control transfers to the next loader stage.</li>
  </ul>
</details>

<details>
  <summary><strong>Stage 1/2 – Loader (16-bit → 32-bit → 64-bit)</strong></summary>
  <ul>
    <li>Loads the kernel image from disk.</li>
    <li>Prepares descriptor tables and the state required for switching modes.</li>
    <li>Enters <strong>32-bit protected mode</strong>, then prepares paging structures and transitions to <strong>64-bit long mode</strong>.</li>
    <li>Provides boot-time video info (framebuffer details) to the kernel.</li>
  </ul>
</details>

<details>
  <summary><strong>Stage 3 – Long mode handoff (64-bit)</strong></summary>
  <ul>
    <li>Long mode is enabled and control jumps to the Rust kernel entry: <code>_start(boot_info)</code>.</li>
    <li>The kernel initializes the framebuffer desktop, input, storage, and networking, then enters the shell/event loop.</li>
  </ul>
</details>

<h3>UEFI boot notes</h3>
<ul>
  <li>UEFI does <em>not</em> use the BIOS “real-mode MBR” boot sequence; instead it loads an EFI executable.</li>
  <li>Othello’s UEFI path still ends the same way: jump into long-mode Rust with a framebuffer description.</li>
</ul>

<hr />

<h2 id="features--design-goals">Features &amp; Design Goals</h2>

<table>
  <thead>
    <tr>
      <th align="left">Educational OS</th>
      <th align="left">Modular Layout</th>
      <th align="left">Desktop + Tools</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td valign="top">
        <ul>
          <li>Exposes every major step of the boot pipeline with <strong>comments over cleverness</strong>.</li>
          <li>Great for self-study, lab assignments, or “I want to see how this really works”.</li>
          <li>Shows how you get from firmware to <strong>Rust</strong> with no OS in between.</li>
        </ul>
      </td>
      <td valign="top">
        <ul>
          <li>Boot/mode switching, interrupts, paging, FS, net, and UI are kept in separate units.</li>
          <li>Easy to swap out scheduler, allocator, or drivers.</li>
          <li>Clear hook points for expanding into multi-tasking or RTOS scheduling.</li>
        </ul>
      </td>
      <td valign="top">
        <ul>
          <li>Framebuffer desktop + terminal shell.</li>
          <li>Basic apps (login, editor, registry viewer, browser).</li>
          <li>Networking stack + HTTP client for real I/O experiments.</li>
        </ul>
      </td>
    </tr>
  </tbody>
</table>

<hr />

<h2 id="desktop--apps">Desktop &amp; Apps</h2>

<p>
  Othello brings up a small framebuffer desktop and runs apps inside it.
  The UI is intentionally simple, but it’s <strong>real</strong> — input, windows, and an event-driven shell.
</p>

<h3>What’s currently there</h3>
<ul>
  <li><strong>Login screen</strong> – user create/login flow (backed by a tiny in-kernel registry).</li>
  <li><strong>Desktop / windowing</strong> – wallpaper + dock/taskbar + window chrome.</li>
  <li><strong>Terminal (Shell)</strong> – interactive CLI rendered into a window; drives most tooling.</li>
  <li><strong>Text editor</strong> – <code>edit &lt;path&gt;</code> opens a file, edits, saves.</li>
  <li><strong>Registry viewer</strong> – inspect stored entries (by design, read-only UI).</li>
  <li><strong>Browser</strong> – omnibox UI; fetches real HTTP and renders a text-first view of HTML.</li>
</ul>

<p align="center">
  <sub><em>Most “apps” are still kernel-resident — the point is to evolve services + UX before introducing user space.</em></sub>
</p>

<hr />

<h2 id="shell--commands">Shell &amp; Commands</h2>

<p>
  Othello’s shell is an interactive terminal rendered inside a desktop window.
  It acts as the primary control surface for the OS: launching apps, inspecting state, configuring networking,
  and interacting with the filesystem.
</p>

<h3>How it works</h3>
<ul>
  <li><strong>Input</strong> comes from the keyboard driver and is routed into the terminal widget.</li>
  <li><strong>Command parsing</strong> is handled by a small dispatcher (filesystem builtins first; then shell builtins).</li>
  <li><strong>Output</strong> is rendered into the terminal buffer (scrollback + prompt) and displayed on the framebuffer.</li>
</ul>

<h3>Commands</h3>
<p><em>Note:</em> names may evolve as the OS changes. This list reflects the current shell surface.</p>

<h4>Core</h4>
<ul>
  <li><code>help</code> – list commands + UI tips</li>
  <li><code>clear</code> – clear the terminal buffer</li>
  <li><code>about</code> – show OS version info</li>
  <li><code>echo &lt;text...&gt;</code> – print text</li>
  <li><code>tsc</code> – print timestamp counter (RDTSC)</li>
</ul>

<h4>Apps</h4>
<ul>
  <li><code>login</code> – lock and return to the login screen</li>
  <li><code>reg</code> – open registry viewer</li>
  <li><code>edit &lt;path&gt;</code> (or <code>notepad</code>) – open text editor (defaults to <code>/home/user/readme.txt</code>)</li>
</ul>

<h4>Networking</h4>
<ul>
  <li><code>net</code> – initialize NIC (RTL8139) and list detected adapters</li>
  <li><code>ipconfig</code> / <code>ifconfig</code> – show current IP configuration</li>
  <li><code>dhcp</code> – attempt to obtain a lease via DHCP</li>
  <li><code>ipset &lt;ip&gt; &lt;mask&gt; &lt;gw&gt; [dns]</code> – set a static IPv4 configuration (<code>ipset qemu</code> supported)</li>
  <li><code>ping &lt;ip&gt; [count]</code> – ICMP ping (and helpful errors if you’re not configured)</li>
</ul>

<h4>Filesystem</h4>
<ul>
  <li><code>pwd</code> – print working directory</li>
  <li><code>cd &lt;path&gt;</code> – change directory</li>
  <li><code>ls</code> – list directory contents</li>
  <li><code>cat &lt;path&gt;</code> – print file contents</li>
  <li><code>mkdir &lt;path&gt;</code> – create directory</li>
  <li><code>touch &lt;path&gt;</code> – create empty file</li>
  <li><code>rm &lt;path&gt;</code> – remove file (and/or directory where supported)</li>
  <li><code>write &lt;path&gt; &lt;text...&gt;</code> – overwrite file with text</li>
  <li><code>append &lt;path&gt; &lt;text...&gt;</code> – append text to a file</li>
</ul>

<h4>Persistence</h4>
<ul>
  <li><code>sync</code> – flush dirty changes to disk (when persistence is enabled)</li>
  <li><code>persist</code> – show persistence status / mount info</li>
</ul>

<hr />

<h2 id="networking--web">Networking &amp; Web</h2>

<p>
  Othello includes a small but practical networking stack intended for clarity and experimentation.
  The goal is to make it easy to trace a packet from the NIC all the way up to a browser fetch.
</p>

<h3>Stack overview</h3>
<ul>
  <li><strong>NIC driver:</strong> RTL8139</li>
  <li><strong>L2/L3:</strong> Ethernet, ARP, IPv4</li>
  <li><strong>L4:</strong> UDP (DHCP, DNS), minimal TCP client support</li>
  <li><strong>Application:</strong> HTTP/1.1 client (used by the browser and testing tools)</li>
</ul>

<h3>HTTP support</h3>
<p>The HTTP client is designed to be usable for real-world pages while staying small:</p>
<ul>
  <li>HTTP/1.1 request/response parsing</li>
  <li>Redirect handling (<code>Location</code>)</li>
  <li>Chunked transfer decoding</li>
  <li>DNS A-resolution for hostnames</li>
</ul>

<h3>HTTPS support</h3>
<ul>
  <li><strong>Native TLS is not implemented in-kernel yet.</strong></li>
  <li>
    For now, <code>https://</code> URLs are fetched via an optional <strong>host-side HTTPS proxy</strong>
    (QEMU user networking default: <code>10.0.2.2:8000</code>).
    This keeps the kernel/networking stack understandable while still enabling HTTPS content during development.
  </li>
</ul>

<h3>Browser integration</h3>
<ul>
  <li>The browser uses DNS/TCP/HTTP for fetching.</li>
  <li>Rendering is currently a <strong>text-first pipeline</strong> (readable extraction from HTML),
      with separate scaffolding (<code>web/</code>) for future HTML/CSS/DOM/layout work.</li>
</ul>

<h3>Testing &amp; troubleshooting</h3>
<ul>
  <li>Use <code>ipconfig</code> to confirm IP, gateway, and DNS.</li>
  <li>Run <code>dhcp</code> to obtain a lease, or <code>ipset</code> for manual configuration.</li>
  <li>Use <code>ping</code> to validate basic connectivity (and DNS if pinging a hostname).</li>
</ul>

<hr />

<h2 id="security">Security</h2>

<p>
  Othello is developed with a security-first mindset and is intended to be <strong>compliant with NIST and NSA standards</strong>
  as a baseline for hardening and secure engineering practices.
  <em>(This is a research/education OS — compliance here is self-attested unless a release explicitly documents a formal audit.)</em>
</p>

<h3>Security goals</h3>
<ul>
  <li><strong>Minimize attack surface</strong> – keep services small, explicit, and auditable.</li>
  <li><strong>Prefer memory-safe implementation</strong> – Rust is used for most kernel and application logic.</li>
  <li><strong>Secure defaults</strong> – strict bounds checks, defensive parsing, and fail-closed behavior.</li>
  <li><strong>Traceable behavior</strong> – consistent error paths + serial logging for analysis during bring-up.</li>
</ul>

<h3>What this means in practice (today)</h3>
<ul>
  <li><strong>Defensive input handling:</strong> network packets and file operations are validated and bounded.</li>
  <li><strong>Reduced complexity:</strong> small protocol surface and minimal dynamic behavior by default.</li>
  <li><strong>Clear trust boundaries:</strong> planned kernel/user separation once tasking and syscalls land.</li>
</ul>

<h3>Hardening roadmap</h3>
<ul>
  <li>Add stronger compartmentalization (user-mode apps, syscall boundary, privilege separation).</li>
  <li>Introduce cryptographic primitives appropriate for OS dev experiments (hashing, signatures) and evaluate a minimal TLS approach.</li>
  <li>Expand auditing/logging around privileged operations (network config changes, persistence writes, auth events).</li>
</ul>

<hr />

<h2 id="filesystem--persistence">Filesystem &amp; Persistence</h2>

<p>
  Othello uses an in-kernel <strong>RAM filesystem</strong> for simplicity, with an optional persistence layer:
  an append-only log stored on disk and replayed into RAM at boot.
</p>

<h3>File commands</h3>
<ul>
  <li><code>pwd</code>, <code>cd</code>, <code>ls</code>, <code>cat</code></li>
  <li><code>mkdir</code>, <code>touch</code>, <code>rm</code></li>
  <li><code>write &lt;path&gt; &lt;text...&gt;</code>, <code>append &lt;path&gt; &lt;text...&gt;</code></li>
</ul>

<h3>Persistence commands</h3>
<ul>
  <li><code>sync</code> – flush dirty changes to disk (when persistence is enabled)</li>
  <li><code>persist</code> – show persistence status / mount info</li>
</ul>

<hr />

<h2 id="web-browser">Web Browser</h2>

<p>
  The browser is deliberately split into two layers:
</p>
<ul>
  <li><strong>Networking + fetch:</strong> DNS + TCP + HTTP client with redirect/chunk handling (HTTPS via proxy).</li>
  <li><strong>Rendering:</strong> currently <strong>text-first</strong> for clarity and stability.</li>
</ul>

<p>
  A lightweight engine scaffold exists under <code>web/</code> (HTML/CSS/DOM/layout/JS stubs) so the renderer can evolve
  from “text view” into a real page layout system over time.
</p>

<hr />

<h2 id="kernel-architecture">Kernel Architecture</h2>

<p>
  The Rust kernel is structured to keep low-level hardware details separate from higher-level logic.
  The exact layout may evolve, but the current architecture is roughly:
</p>

<pre><code>Rust-Kernel/
├─ src/
│  ├─ rust-kernel.rs          # kernel entry: _start(boot_info)
│  ├─ bootinfo.rs             # boot-time payload helpers
│  ├─ serial.rs               # serial logging (early debug)
│  ├─ portio.rs               # x86 I/O helpers
│  ├─ idt.rs                  # IDT + exception/IRQ glue
│  ├─ heap.rs                 # heap init
│  ├─ framebuffer_driver.rs   # framebuffer + drawing primitives
│  ├─ keyboard.rs / mouse.rs  # input
│  ├─ gui.rs                  # desktop + windows + dock/taskbar
│  ├─ shell.rs                # terminal window + command dispatcher
│  ├─ fs.rs / fs_cmds.rs      # RAM FS + shell commands
│  ├─ persist.rs              # append-only persistence log (optional)
│  ├─ net.rs                  # RTL8139 + core networking
│  ├─ net/                    # DNS, TCP, HTTP, TLS placeholder
│  ├─ browser.rs              # browser UI + fetch + text rendering
│  ├─ editor.rs               # text editor
│  ├─ login.rs                # login UI + user creation
│  ├─ registry.rs / regedit.rs# tiny registry + viewer
│  └─ web/                    # HTML/CSS/DOM/layout/JS scaffolding
└─ Cargo.toml</code></pre>

<p>
  The kernel entry (<code>_start</code>) is called after the loader finishes mode switching and passes framebuffer details.
  A typical initialization flow is:
</p>

<ol>
  <li>Initialize early debug output; set up interrupts/IDT.</li>
  <li>Bring up framebuffer primitives and paint a visible UI quickly (login/desktop).</li>
  <li>Initialize heap, RAM FS, and optional persistence replay/mount.</li>
  <li>Initialize input devices and the network stack.</li>
  <li>Enter the shell event loop (which also drives UI/app switching).</li>
</ol>

<hr />

<h2 id="memory-model">Memory Model &amp; Paging</h2>

<p>
  Othello starts with a <strong>flat memory model</strong> plus <strong>identity-mapped paging</strong> to keep early debugging simple:
</p>

<ul>
  <li>Segment bases are effectively 0 (flat segmentation).</li>
  <li>Virtual address <code>0x0000_1234</code> maps to physical <code>0x0000_1234</code> in early stages.</li>
  <li>Paging is still enabled, so you get:
    <ul>
      <li>protection bits (R/W, user/supervisor),</li>
      <li>faults you can hook for debugging.</li>
    </ul>
  </li>
</ul>

<h3>Future / experimental directions</h3>
<ul>
  <li><strong>Higher-half kernel</strong> – relocate the kernel into the higher half of the virtual address space.</li>
  <li><strong>User space vs kernel space</strong> – split address space into distinct privilege domains for isolation.</li>
  <li><strong>Per-task address spaces</strong> – give each process its own page tables for stronger isolation and experimentation with multi-tasking.</li>
</ul>

<p>
  The paging subsystem is a natural place to experiment with advanced concepts like copy-on-write, guard pages, and memory-mapped I/O once the basics are solid.
</p>

<hr />

<h2 id="repo-layout">Repository Layout</h2>

<p>The high-level structure of the project:</p>

<pre><code>Othello-OS/
├─ OS_Build/                 # Build orchestration and boot/ISO pipeline (if present)
│  ├─ BUILDING.md             # Detailed build instructions and tooling
│  ├─ boot/                   # MBR, stage-2 loader, linker scripts, etc.
│  └─ scripts/                # Helper scripts (build, run, ISO packing)
├─ Rust-Kernel/               # Rust kernel crate (desktop, shell, net, FS)
│  ├─ src/
│  └─ Cargo.toml
├─ tools/                     # Optional helpers (e.g., HTTPS proxy for QEMU workflows)
├─ docs/                      # Optional design notes / diagrams (if present)
└─ README.md                  # You are here</code></pre>

<p>
  The exact layout may evolve; <code>OS_Build/BUILDING.md</code> is the canonical reference for the build system and toolchain expectations.
</p>

<hr />

<h2 id="getting-started">Getting Started</h2>

<h3>Prerequisites</h3>
<p>You’ll typically want:</p>
<ul>
  <li><strong>Rust</strong> (with a bare-metal target like <code>x86_64-unknown-none</code>)</li>
  <li>An <strong>assembler</strong> (e.g. <code>nasm</code> / <code>yasm</code>)</li>
  <li>A <strong>C toolchain</strong> (<code>gcc</code>, <code>clang</code>, etc.)</li>
  <li><strong>QEMU</strong> or another x86_64 hypervisor/emulator</li>
  <li>Shell environment:
    <ul>
      <li>Bash / Zsh on Linux/macOS, or</li>
      <li>PowerShell on Windows</li>
    </ul>
  </li>
</ul>

<p>
  For exact version hints and platform notes, see <a href="OS_Build/BUILDING.md"><code>OS_Build/BUILDING.md</code></a>.
</p>

<h3>Clone &amp; build</h3>

<pre><code># Clone the repository
git clone https://github.com/&lt;your-user&gt;/Othello-OS.git
cd Othello-OS

# Go to the build orchestration directory
cd OS_Build

# Build the disk image and run it (Linux/macOS)
./build-and-run.sh

# Or on Windows (PowerShell)
# ./build-and-run.ps1</code></pre>

<p>Typical build scripts will:</p>
<ol>
  <li>Assemble the bootloader (MBR + stage-2 or UEFI loader).</li>
  <li>Compile the Rust/C kernel.</li>
  <li>Link everything into a flat binary.</li>
  <li>Produce a bootable disk image or El Torito ISO.</li>
  <li>Launch QEMU with that image.</li>
</ol>

<hr />

<h2 id="hacking-points">Hacking Points</h2>

<p>Use Othello as a <strong>sandbox</strong> rather than a black box. Good entry points:</p>

<h3>Bootloader &amp; mode switching</h3>
<ul>
  <li><code>BEGIN_32BIT</code> – where CR0 gets flipped and the CPU enters protected mode.</li>
  <li><code>BEGIN_64BIT</code> – where long mode paging is set up and control jumps to 64-bit code.</li>
  <li>Boot payload handoff – experiment with framebuffer formats, memory maps, and kernel virtual layouts.</li>
</ul>

<h3>Shell + desktop</h3>
<ul>
  <li><code>shell.rs</code> – command parsing, terminal rendering, event loop.</li>
  <li><code>gui.rs</code> – windowing, dock/taskbar, painting and compositing.</li>
  <li><code>login.rs</code> – auth UI + user flows.</li>
</ul>

<h3>Networking &amp; HTTP</h3>
<ul>
  <li><code>net.rs</code> – RTL8139 + core protocols (ARP/IPv4/UDP/DHCP/ICMP).</li>
  <li><code>net/dns.rs</code> – DNS A queries.</li>
  <li><code>net/tcp.rs</code> – minimal TCP client.</li>
  <li><code>net/http.rs</code> – HTTP client (redirects, chunked decode) + HTTPS proxy path.</li>
</ul>

<h3>Filesystem &amp; persistence</h3>
<ul>
  <li><code>fs.rs</code> / <code>fs_cmds.rs</code> – RAM FS and shell commands.</li>
  <li><code>persist.rs</code> – on-disk append-only log, replay at boot, <code>sync</code> for flushing changes.</li>
</ul>

<h3>Browser + renderer scaffolding</h3>
<ul>
  <li><code>browser.rs</code> – omnibox UI + fetch + text rendering pipeline.</li>
  <li><code>web/</code> – HTML/CSS/DOM/layout/JS scaffolding for future “real” rendering.</li>
</ul>

<hr />

<h2 id="roadmap--status">Roadmap &amp; Status</h2>

<p>
  This is a <strong>research / education OS</strong>, not a production operating system.
  Expect rough edges &amp; experimental branches.
</p>

<h3>Short-term</h3>
<ul>
  <li>Polish boot-stage logs and error codes.</li>
  <li>Stabilize shell commands and UI behaviors (window focus, switching, docking).</li>
  <li>Harden interrupt/exception handling paths.</li>
  <li>Improve DHCP reliability + add better network diagnostics.</li>
</ul>

<h3>Medium-term</h3>
<ul>
  <li>Add a minimal scheduler + task abstraction.</li>
  <li>Expand filesystem capabilities (metadata, directories, persistence compaction).</li>
  <li>Upgrade the browser from “text view” toward real layout (HTML/CSS box model + images).</li>
  <li>Improve HTTP robustness (more headers, better streaming, caching primitives).</li>
</ul>

<h3>Long-term / Experimental</h3>
<ul>
  <li>Explore RTOS-style scheduling for soft real-time workloads.</li>
  <li>Bring up multi-core (SMP) and inter-CPU communication.</li>
  <li>Implement more advanced memory management:
    <ul>
      <li>higher-half kernel,</li>
      <li>isolated user space,</li>
      <li>per-process virtual memory.</li>
    </ul>
  </li>
  <li>Native TLS in-kernel (or a minimal crypto/TLS stack suitable for OSdev experiments).</li>
</ul>

<hr />

<h2 id="faq">FAQ</h2>

<h3>Is Othello meant to be a daily-driver OS?</h3>
<p>No. It’s a <strong>research and learning</strong> OS. It aims to be small, understandable, and hackable, not full-featured.</p>

<h3>Can I use Othello as a base for my own kernel?</h3>
<p>
  Possibly! Check the <a href="#license">license</a> section and the <code>LICENSE</code> file, and consider opening an issue if you plan to build something substantial on top of it.
</p>

<h3>Does it support UEFI?</h3>
<p>Yes. Othello supports <strong>BIOS</strong> and <strong>UEFI</strong> bootloading paths (both custom built).</p>

<h3>Does it run on real hardware?</h3>
<p>Yes, this OS is primarily intended to run on bare-metal. Though, it can also run as a virtual machine in QEMU.</p>

<hr />

<h2 id="contributing">Contributing</h2>

<p>
  Contributions that improve <strong>clarity</strong>, <strong>documentation</strong>, or <strong>low-level tooling</strong> are very welcome:
</p>

<ul>
  <li>Keep PRs small and focused (one subsystem or concern at a time).</li>
  <li>Favor comments that explain <strong>why</strong> a low-level sequence exists, not just what it does.</li>
  <li>For larger architectural changes, please start a discussion in an issue first.</li>
</ul>

<p>
  If you use Othello in a course, lab, or research project, feel free to open an issue with a short write-up — it’s genuinely helpful to know what the OS is being used for.
</p>

<h2 id="code-of-conduct">Code of Conduct</h2>

<p>
  This project is governed by a <strong>Code of Conduct</strong>. By participating in the issue tracker, discussions, or pull requests, you agree to follow it.
</p>

<p>See: <a href="CODE_OF_CONDUCT.md"><code>CODE_OF_CONDUCT.md</code></a></p>

<h2 id="license">License</h2>

<p>
  This project is licensed under the terms specified in the <a href="LICENSE"><code>LICENSE</code></a> file in this repository.
</p>

<p>
  You are encouraged to read the license before reusing or redistributing any part of the code, and to reference this repository if you build derivative or educational material on top of it.
</p>
