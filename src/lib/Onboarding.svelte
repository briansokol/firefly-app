<script lang="ts">
  import {
    signup,
    login,
    registerDevice,
    claimDevice,
    type DeviceSummary,
    type Profile,
  } from "$lib/api";

  let { ondone }: { ondone: (profiles: Profile[]) => void } = $props();

  type Step = "choice" | "createAccount" | "createDevice" | "signin" | "signinDevice";
  let step = $state<Step>("choice");

  let username = $state("");
  let password = $state("");
  let displayName = $state("");
  let deviceName = $state("");
  let devices = $state<DeviceSummary[]>([]);
  let selectedDeviceId = $state<string | null>(null);
  let registerNew = $state(false);

  let busy = $state(false);
  let error = $state<string | null>(null);

  function goto(next: Step) {
    error = null;
    step = next;
  }

  async function doSignup() {
    busy = true;
    error = null;
    try {
      await signup(username.trim(), password, displayName.trim());
      goto("createDevice");
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function doLogin() {
    busy = true;
    error = null;
    try {
      devices = await login(username.trim(), password);
      registerNew = devices.length === 0;
      selectedDeviceId = null;
      goto("signinDevice");
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function doRegisterDevice() {
    busy = true;
    error = null;
    try {
      ondone(await registerDevice(deviceName.trim()));
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function doClaim() {
    if (!selectedDeviceId) return;
    busy = true;
    error = null;
    try {
      ondone(await claimDevice(selectedDeviceId));
    } catch (e) {
      error = String(e); // stay on step on 404 "unknown device"
    } finally {
      busy = false;
    }
  }
</script>

<div class="onboard-screen">
  <div class="onboard ff-card">
    <div class="brand brand--lg"><span class="ff-spark"></span> Firefly</div>

    {#if error}
      <p class="onboard-error">{error}</p>
    {/if}

    {#if step === "choice"}
      <h2>Welcome</h2>
      <p>Create a new account, or sign in to one you already have.</p>
      <button class="ff-btn ff-btn--primary" onclick={() => goto("createAccount")}>Create account</button>
      <button class="ff-btn" onclick={() => goto("signin")}>Sign in</button>
    {:else if step === "createAccount"}
      <h2>Create account</h2>
      <p>New accounts start as a kid profile. An adult can upgrade it later on the server.</p>
      <input class="ff-input" placeholder="Username" autocomplete="username" bind:value={username} />
      <input
        class="ff-input"
        type="password"
        placeholder="Password"
        autocomplete="new-password"
        bind:value={password}
      />
      <p class="onboard-hint">Password must be at least 8 characters.</p>
      <input class="ff-input" placeholder="Display name" bind:value={displayName} />
      <button
        class="ff-btn ff-btn--primary"
        disabled={busy || !username.trim() || password.length < 8 || !displayName.trim()}
        onclick={doSignup}>{busy ? "Creating…" : "Continue"}</button
      >
      <button class="ff-btn ff-btn--ghost" onclick={() => goto("choice")}>Back</button>
    {:else if step === "createDevice"}
      <h2>Name this device</h2>
      <p>Give this install a name so you can recognize it across your devices.</p>
      <input class="ff-input" placeholder="e.g. MacBook" bind:value={deviceName} />
      <button
        class="ff-btn ff-btn--primary"
        disabled={busy || !deviceName.trim()}
        onclick={doRegisterDevice}>{busy ? "Setting up…" : "Finish"}</button
      >
    {:else if step === "signin"}
      <h2>Sign in</h2>
      <input class="ff-input" placeholder="Username" autocomplete="username" bind:value={username} />
      <input
        class="ff-input"
        type="password"
        placeholder="Password"
        autocomplete="current-password"
        bind:value={password}
      />
      <button
        class="ff-btn ff-btn--primary"
        disabled={busy || !username.trim() || !password}
        onclick={doLogin}>{busy ? "Signing in…" : "Continue"}</button
      >
      <button class="ff-btn ff-btn--ghost" onclick={() => goto("choice")}>Back</button>
    {:else if step === "signinDevice"}
      <h2>Choose a device</h2>
      {#if devices.length > 0}
        <p>Reuse an existing device on this install, or register a new one.</p>
        <div class="ff-radios">
          {#each devices as d (d.id)}
            <label class="ff-radio">
              <input
                type="radio"
                name="device"
                checked={!registerNew && selectedDeviceId === d.id}
                onchange={() => {
                  registerNew = false;
                  selectedDeviceId = d.id;
                }}
              />
              <span>{d.name}{#if d.lastSync} · last sync {d.lastSync}{/if}</span>
            </label>
          {/each}
          <label class="ff-radio">
            <input
              type="radio"
              name="device"
              checked={registerNew}
              onchange={() => {
                registerNew = true;
                selectedDeviceId = null;
              }}
            />
            <span>Register a new device</span>
          </label>
        </div>
      {:else}
        <p>No existing devices on this account. Register this install as your first device.</p>
      {/if}

      {#if registerNew}
        <input class="ff-input" placeholder="Device name (e.g. iPad)" bind:value={deviceName} />
        <button
          class="ff-btn ff-btn--primary"
          disabled={busy || !deviceName.trim()}
          onclick={doRegisterDevice}>{busy ? "Setting up…" : "Finish"}</button
        >
      {:else}
        <button
          class="ff-btn ff-btn--primary"
          disabled={busy || !selectedDeviceId}
          onclick={doClaim}>{busy ? "Connecting…" : "Use this device"}</button
        >
      {/if}
      <button class="ff-btn ff-btn--ghost" onclick={() => goto("signin")}>Back</button>
    {/if}
  </div>
</div>

<style>
  .onboard-screen {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
    background:
      radial-gradient(800px 400px at 50% 120%, rgba(139, 92, 246, 0.1), transparent 60%),
      var(--ff-surface-app);
  }
  .onboard {
    width: 380px;
    max-width: 100%;
    display: flex;
    flex-direction: column;
    gap: 16px;
    box-shadow: var(--ff-shadow-pop);
  }
  .brand--lg {
    font-size: 22px;
  }
  .onboard h2 {
    margin: 0;
    font: 600 24px var(--ff-font-display);
    color: var(--ff-text-body);
  }
  .onboard p {
    margin: 0;
    font-size: var(--ff-text-base);
    color: var(--ff-text-muted);
    line-height: var(--ff-leading-body);
  }
  .onboard :global(.ff-btn) {
    align-self: flex-start;
  }
  .onboard-hint {
    margin-top: -8px;
    font-size: var(--ff-text-sm);
  }
  .onboard-error {
    margin: 0;
    color: var(--ff-red);
    font-size: var(--ff-text-base);
    font-weight: 700;
  }
</style>
