import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type SandboxProfileInfo = {
  instanceId: string;
  profileName: string;
  sid: string;
  created: boolean;
};

export default function App() {
  const [result, setResult] =
    useState<SandboxProfileInfo | null>(null);

  const [error, setError] =
    useState<string | null>(null);

  const createProfile = async () => {
    setError(null);

    try {
      const profile =
        await invoke<SandboxProfileInfo>(
          "ensure_sandbox_profile",
          {
            instanceId: "sandbox-test",
          },
        );

      setResult(profile);
    } catch (cause) {
      setError(String(cause));
    }
  };

  return (
    <main>
      <h1>MonaLauncher Sandbox Lab</h1>

      <button
        type="button"
        onClick={createProfile}
      >
        Ensure AppContainer profile
      </button>

      {error && (
        <pre>{error}</pre>
      )}

      {result && (
        <dl>
          <dt>Instance ID</dt>
          <dd>{result.instanceId}</dd>

          <dt>Profile name</dt>
          <dd>{result.profileName}</dd>

          <dt>SID</dt>
          <dd>{result.sid}</dd>

          <dt>Status</dt>
          <dd>
            {result.created
              ? "Created"
              : "Already existed"}
          </dd>
        </dl>
      )}
    </main>
  );
}
