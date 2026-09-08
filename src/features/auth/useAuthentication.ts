import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useRef, useState } from "react";
import type {
  MicrosoftAuthStatus,
  MicrosoftSignInChallenge,
  MicrosoftSignInPoll,
  MinecraftAccountProfile,
} from "../../domain/launcher";
import { hasTauriRuntime } from "../../lib/tauri";
export function useAuthentication() {
  const [authStatus, setAuthStatus] = useState<MicrosoftAuthStatus>({
    configured: false,
    authorized: false,
  });

  const [showAuth, setShowAuth] = useState(false);

  const [authChallenge, setAuthChallenge] = useState<MicrosoftSignInChallenge | null>(null);

  const [authBusy, setAuthBusy] = useState<"begin" | "signout" | null>(null);

  const [authError, setAuthError] = useState<string | null>(null);

  const [minecraftProfile, setMinecraftProfile] = useState<MinecraftAccountProfile | null>(null);

  const [minecraftProfileLoading, setMinecraftProfileLoading] = useState(false);

  const authRequestGenerationRef = useRef(0);

  const openAuth = () => {
    setAuthError(null);
    setShowAuth(true);
  };

  const refreshMinecraftProfile = async () => {
    const generation = authRequestGenerationRef.current;
    setMinecraftProfileLoading(true);
    try {
      const profile = await invoke<MinecraftAccountProfile>("refresh_minecraft_account");
      if (authRequestGenerationRef.current !== generation) return;
      setMinecraftProfile(profile);
      setAuthError(null);
    } catch (cause) {
      if (authRequestGenerationRef.current !== generation) return;
      setMinecraftProfile(null);
      setAuthError(String(cause));
    } finally {
      if (authRequestGenerationRef.current === generation) setMinecraftProfileLoading(false);
    }
  };

  const refreshAuthStatus = async () => {
    const generation = authRequestGenerationRef.current;
    const status = await invoke<MicrosoftAuthStatus>("microsoft_auth_status");
    if (authRequestGenerationRef.current !== generation) return;
    setAuthStatus(status);
    if (status.authorized) void refreshMinecraftProfile();
  };

  useEffect(() => {
    if (!authChallenge || authStatus.authorized) return;

    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const poll = async () => {
      try {
        const result = await invoke<MicrosoftSignInPoll>("poll_microsoft_sign_in", {
          sessionId: authChallenge.sessionId,
        });
        if (cancelled) return;
        if (result.status === "authorized") {
          authRequestGenerationRef.current += 1;
          setMinecraftProfileLoading(false);
          setMinecraftProfile(null);
          setAuthStatus((current) => ({ ...current, authorized: true }));
          setAuthChallenge(null);
          setAuthError(null);
          void refreshMinecraftProfile();
          return;
        }
        timer = setTimeout(poll, (result.retryAfter ?? authChallenge.interval) * 1000);
      } catch (cause) {
        if (!cancelled) {
          setAuthError(String(cause));
          setAuthChallenge(null);
        }
      }
    };

    timer = setTimeout(poll, authChallenge.interval * 1000);
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [authChallenge, authStatus.authorized]);

  const beginMicrosoftSignIn = async () => {
    setAuthBusy("begin");
    setAuthError(null);
    try {
      const challenge = await invoke<MicrosoftSignInChallenge>("begin_microsoft_sign_in");
      setAuthChallenge(challenge);
      await openMicrosoftVerification(challenge.verificationUri);
    } catch (cause) {
      setAuthError(String(cause));
    } finally {
      setAuthBusy(null);
    }
  };

  const openMicrosoftVerification = async (verificationUri: string) => {
    try {
      await openUrl(verificationUri);
    } catch (cause) {
      setAuthError(`ブラウザーを開けませんでした。下のURLを手動で開いてください: ${cause}`);
    }
  };

  const signOutMicrosoft = async () => {
    authRequestGenerationRef.current += 1;
    setMinecraftProfileLoading(false);
    setAuthBusy("signout");
    setAuthError(null);
    try {
      await invoke("sign_out_microsoft");
      setAuthChallenge(null);
      setMinecraftProfile(null);
      setAuthStatus((current) => ({ ...current, authorized: false }));
    } catch (cause) {
      setAuthError(String(cause));
    } finally {
      setAuthBusy(null);
    }
  };
  useEffect(() => {
    if (hasTauriRuntime()) void refreshAuthStatus().catch((cause) => setAuthError(String(cause)));
  }, []);
  return {
    authStatus,
    showAuth,
    setShowAuth,
    authChallenge,
    authBusy,
    authError,
    minecraftProfile,
    minecraftProfileLoading,
    openAuth,
    refreshMinecraftProfile,
    beginMicrosoftSignIn,
    openMicrosoftVerification,
    signOutMicrosoft,
  };
}
