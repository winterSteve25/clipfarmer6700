import { useState } from "react";
import type { PublisherAccount, PublisherCredentials, PublisherPlatform } from "../types";
import { Icon } from "../components/Icon";

type Props = {
  accounts: PublisherAccount[];
  busyPlatform: PublisherPlatform | null;
  onConnect: (platform: PublisherPlatform, credentials: PublisherCredentials) => Promise<boolean>;
  onDisconnect: (platform: PublisherPlatform) => Promise<void>;
};

const PLATFORMS: Array<{
  id: PublisherPlatform;
  name: string;
  monogram: string;
  description: string;
  tokenLabel: string;
  extra?: { key: "accountId" | "clientId"; label: string; placeholder: string };
}> = [
  { id: "youtube", name: "YouTube", monogram: "YT", description: "Publish finished clips as YouTube Shorts.", tokenLabel: "OAuth access token" },
  { id: "tiktok", name: "TikTok", monogram: "TT", description: "Send clips to your TikTok creator inbox as drafts.", tokenLabel: "Content Posting API access token" },
  { id: "instagram", name: "Instagram", monogram: "IG", description: "Publish vertical clips to Instagram Reels.", tokenLabel: "Graph API access token", extra: { key: "accountId", label: "Instagram account ID", placeholder: "17841400000000000" } },
  { id: "twitch", name: "Twitch", monogram: "TW", description: "Create Twitch clips from detected moments.", tokenLabel: "User access token", extra: { key: "clientId", label: "Application client ID", placeholder: "Your Twitch application client ID" } },
];

const EMPTY_CREDENTIALS: PublisherCredentials = { accessToken: "", accountId: "", clientId: "" };

export function SettingsView({ accounts, busyPlatform, onConnect, onDisconnect }: Props) {
  const [editing, setEditing] = useState<PublisherPlatform | null>(null);
  const [credentials, setCredentials] = useState<PublisherCredentials>(EMPTY_CREDENTIALS);

  function beginConnect(platform: PublisherPlatform) {
    setEditing(platform);
    setCredentials(EMPTY_CREDENTIALS);
  }

  async function save(platform: PublisherPlatform) {
    if (await onConnect(platform, credentials)) {
      setEditing(null);
      setCredentials(EMPTY_CREDENTIALS);
    }
  }

  return <section className="settings-view" aria-labelledby="connected-accounts-heading">
    <div className="settings-intro">
      <span className="eyebrow">PUBLISHING ACCOUNTS</span>
      <h2 id="connected-accounts-heading">Connected accounts</h2>
      <p>Connect each destination you want ClipFarmer to publish to. Credentials are stored in the app's private local data folder and are never shown again after saving.</p>
    </div>

    <div className="account-list">
      {PLATFORMS.map((platform) => {
        const connected = accounts.find((account) => account.platform === platform.id)?.connected ?? false;
        const isEditing = editing === platform.id;
        const busy = busyPlatform === platform.id;
        return <article className={`account-card ${connected ? "connected" : ""}`} key={platform.id}>
          <div className="account-summary">
            <span className={`platform-icon ${platform.id === "tiktok" ? "tiktokDrafts" : platform.id === "twitch" ? "twitchClips" : platform.id}`}>{platform.monogram}</span>
            <div className="account-copy"><div><h3>{platform.name}</h3><span className={`connection-state ${connected ? "connected" : ""}`}><i/>{connected ? "Connected" : "Not connected"}</span></div><p>{platform.description}</p></div>
            {connected
              ? <button className="disconnect-button" type="button" disabled={busy} onClick={() => onDisconnect(platform.id)}>{busy ? "Disconnecting…" : "Disconnect"}</button>
              : <button className="connect-button" type="button" aria-expanded={isEditing} onClick={() => isEditing ? setEditing(null) : beginConnect(platform.id)}>{isEditing ? "Cancel" : "Connect"}</button>}
          </div>

          {isEditing && !connected && <div className="account-form">
            <div className="account-form-note"><Icon name="settings" size={16}/><span>Enter credentials created for ClipFarmer in your {platform.name} developer account.</span></div>
            <label><span>{platform.tokenLabel}</span><input type="password" autoComplete="off" value={credentials.accessToken} onChange={(event) => setCredentials((current) => ({ ...current, accessToken: event.target.value }))} placeholder="Paste token"/></label>
            {platform.extra && <label><span>{platform.extra.label}</span><input type="text" autoComplete="off" value={credentials[platform.extra.key] ?? ""} onChange={(event) => setCredentials((current) => ({ ...current, [platform.extra!.key]: event.target.value }))} placeholder={platform.extra.placeholder}/></label>}
            <div className="account-form-actions"><small>Saved locally with owner-only file permissions.</small><button className="connect-button" type="button" disabled={busy || !credentials.accessToken.trim() || Boolean(platform.extra && !(credentials[platform.extra.key] ?? "").trim())} onClick={() => save(platform.id)}>{busy ? "Connecting…" : `Connect ${platform.name}`}</button></div>
          </div>}
        </article>;
      })}
    </div>
  </section>;
}
