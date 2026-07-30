import { useCallback, useEffect, useState } from "react"
import type { ReactNode } from "react"
import {
  BanIcon,
  CalendarPlusIcon,
  LaptopIcon,
  LogOutIcon,
  RefreshCwIcon,
  ShieldIcon,
  Trash2Icon,
  UserIcon,
} from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import {
  adminDeleteAccount,
  adminListAccounts,
  adminSetAccess,
  adminSetOfflineLeaseHours,
  adminSetSuspended,
  deleteOwnAccount,
  fetchIsAdmin,
  requestAccountCancellation,
  type AdminAccountRow,
} from "@/lib/supabase/account"
import {
  fetchMyBillingSummary,
  formatSubscriptionStatusLabel,
  canCancelSubscription,
  canActivateSubscriptionEarly,
  isSubscriptionCancelScheduled,
  type BillingSummary,
} from "@/lib/supabase/billing"
import {
  buildCancellationEmailOptions,
  openSupportEmail,
} from "@/lib/support-contact"
import { useVerificationStore } from "@/stores/verification-store"
import { ManageSubscriptionButton } from "@/components/billing/ManageSubscriptionButton"
import { CancelSubscriptionButton } from "@/components/billing/CancelSubscriptionButton"
import { ActivateSubscriptionButton } from "@/components/billing/ActivateSubscriptionButton"
import { PaddleSubscribePanel } from "@/components/billing/PaddleSubscribePanel"
import { AnnouncementsAdminPanel } from "@/components/settings/sections/AnnouncementsAdminPanel"
import {
  adminListDevices,
  adminSetDeviceStatus,
  approveOwnDevice,
  deactivateOwnDevice,
  listOwnDevices,
  type DeviceActivation,
} from "@/lib/supabase/devices"

const ACCESS_EXTENSION_OPTIONS = [
  { days: 30, label: "Extend 30 days", toastLabel: "30 days" },
  { days: 365, label: "Extend 1 year", toastLabel: "1 year" },
] as const

const OFFLINE_LEASE_OPTIONS = [
  { hours: 24, label: "24h" },
  { hours: 72, label: "72h" },
  { hours: 168, label: "7d" },
] as const

function formatTimestamp(value: string | null): string {
  if (!value) return "never"
  const parsed = new Date(value)
  return Number.isNaN(parsed.getTime()) ? "unknown" : parsed.toLocaleString()
}

function formatAccessExpiry(value: string | null, isAdmin: boolean): string {
  if (isAdmin) return "admin exempt"
  if (!value) return "not active"

  const parsed = new Date(value)
  if (Number.isNaN(parsed.getTime())) return "unknown"

  return parsed.getTime() <= Date.now()
    ? `ended ${parsed.toLocaleString()}`
    : `until ${parsed.toLocaleString()}`
}

function DeviceStatusBadge({ status }: { status: DeviceActivation["status"] }) {
  const classes =
    status === "approved"
      ? "bg-emerald-500/15 text-emerald-700 dark:text-emerald-300"
      : status === "pending"
        ? "bg-amber-500/15 text-amber-700 dark:text-amber-300"
        : "bg-destructive/15 text-destructive"
  return (
    <span
      className={`rounded px-1.5 py-0.5 text-[10px] tracking-wide uppercase ${classes}`}
    >
      {status}
    </span>
  )
}

function DeviceSummary({
  device,
  isCurrent = false,
}: {
  device: DeviceActivation
  isCurrent?: boolean
}) {
  return (
    <div className="min-w-0">
      <div className="flex flex-wrap items-center gap-2">
        <p className="truncate text-sm font-medium">
          {device.label ?? `${device.os ?? "Unknown"} computer`}
        </p>
        {isCurrent ? (
          <span className="rounded bg-primary/15 px-1.5 py-0.5 text-[10px] tracking-wide text-primary uppercase">
            This computer
          </span>
        ) : null}
        <DeviceStatusBadge status={device.status} />
      </div>
      <p className="text-xs text-muted-foreground">
        {device.os ?? "Unknown OS"}
        {device.appVersion ? ` · SabbathCue ${device.appVersion}` : ""}
      </p>
      <p className="text-xs text-muted-foreground">
        Last seen {formatTimestamp(device.lastSeenAt)}
      </p>
    </div>
  )
}

function OwnDevicesPanel({
  onCurrentDeactivated,
}: {
  onCurrentDeactivated: () => Promise<void>
}) {
  const currentDeviceId = useVerificationStore((s) => s.verifiedDeviceId)
  const currentUserId = useVerificationStore((s) => s.verifiedUserId)
  const [devices, setDevices] = useState<DeviceActivation[]>([])
  const [loading, setLoading] = useState(true)
  const [message, setMessage] = useState<string | null>(null)
  const [busyDeviceId, setBusyDeviceId] = useState<string | null>(null)

  const loadDevices = useCallback(async () => {
    setLoading(true)
    const result = await listOwnDevices()
    if (result.ok) {
      setDevices(result.devices)
      setMessage(null)
    } else {
      setMessage(result.message)
    }
    setLoading(false)
  }, [])

  useEffect(() => {
    let cancelled = false
    void listOwnDevices().then((result) => {
      if (cancelled) return
      if (result.ok) {
        setDevices(result.devices)
        setMessage(null)
      } else {
        setMessage(result.message)
      }
      setLoading(false)
    })
    return () => {
      cancelled = true
    }
  }, [])

  async function deactivate(device: DeviceActivation) {
    setBusyDeviceId(device.deviceId)
    const result = await deactivateOwnDevice(device.deviceId)
    setBusyDeviceId(null)
    if (!result.ok) {
      toast.error(result.message)
      return
    }
    toast.success("Computer deactivated.")
    if (device.deviceId === currentDeviceId) {
      await onCurrentDeactivated()
      return
    }
    await loadDevices()
  }

  async function approve(device: DeviceActivation) {
    if (!currentUserId) return
    setBusyDeviceId(device.deviceId)
    const result = await approveOwnDevice(currentUserId, device.deviceId)
    setBusyDeviceId(null)
    if (!result.ok) {
      toast.error(result.message)
      return
    }
    toast.success("Computer approved.")
    await loadDevices()
  }

  const currentIsApproved = devices.some(
    (device) =>
      device.deviceId === currentDeviceId && device.status === "approved"
  )

  return (
    <div className="glass-panel space-y-3 p-4">
      <div className="flex items-center justify-between gap-3">
        <div>
          <p className="text-sm font-medium">Activated computers</p>
          <p className="text-xs text-muted-foreground">
            Up to two approved computers. Deactivated computers cannot reconnect
            without approval.
          </p>
        </div>
        <Button
          variant="outline"
          size="sm"
          disabled={loading}
          onClick={() => void loadDevices()}
        >
          <RefreshCwIcon className="mr-1.5 size-3.5" />
          Refresh
        </Button>
      </div>
      {loading ? (
        <p className="text-xs text-muted-foreground">Loading computers...</p>
      ) : message ? (
        <p className="text-xs text-destructive">{message}</p>
      ) : (
        <div className="space-y-2">
          {devices.map((device) => (
            <div
              key={device.deviceId}
              className="flex items-center justify-between gap-3 rounded-lg border border-[var(--border-subtle)] bg-[var(--shell-bg-sunken)] p-3"
            >
              <DeviceSummary
                device={device}
                isCurrent={device.deviceId === currentDeviceId}
              />
              {device.status === "pending" && currentIsApproved ? (
                <Button
                  variant="outline"
                  size="sm"
                  disabled={busyDeviceId === device.deviceId}
                  onClick={() => void approve(device)}
                >
                  Approve
                </Button>
              ) : device.status === "revoked" ? null : (
                <Button
                  variant="outline"
                  size="sm"
                  disabled={busyDeviceId === device.deviceId}
                  onClick={() => void deactivate(device)}
                >
                  {device.deviceId === currentDeviceId
                    ? "Deactivate and sign out"
                    : "Deactivate"}
                </Button>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

function AdminDeviceManager({ userId }: { userId: string }) {
  const [devices, setDevices] = useState<DeviceActivation[] | null>(null)
  const [loading, setLoading] = useState(false)
  const [busyDeviceId, setBusyDeviceId] = useState<string | null>(null)

  const loadDevices = useCallback(async () => {
    setLoading(true)
    const result = await adminListDevices(userId)
    setLoading(false)
    if (result.ok) setDevices(result.devices)
    else toast.error(result.message)
  }, [userId])

  async function setStatus(deviceId: string, status: "approved" | "revoked") {
    setBusyDeviceId(deviceId)
    const result = await adminSetDeviceStatus(userId, deviceId, status)
    setBusyDeviceId(null)
    if (!result.ok) {
      toast.error(result.message)
      return
    }
    toast.success(
      status === "approved" ? "Computer approved." : "Computer revoked."
    )
    await loadDevices()
  }

  if (devices === null) {
    return (
      <Button
        variant="ghost"
        size="sm"
        disabled={loading}
        onClick={() => void loadDevices()}
      >
        <LaptopIcon className="mr-1.5 size-3.5" />
        {loading ? "Loading computers..." : "Manage computers"}
      </Button>
    )
  }

  return (
    <div className="space-y-2 border-t border-[var(--border-subtle)] pt-2">
      {devices.map((device) => (
        <div
          key={device.deviceId}
          className="flex items-center justify-between gap-3 rounded-md bg-[var(--shell-bg-sunken)] p-2"
        >
          <DeviceSummary device={device} />
          <div className="flex gap-2">
            {device.status === "approved" ? (
              <Button
                variant="outline"
                size="sm"
                disabled={busyDeviceId === device.deviceId}
                onClick={() => void setStatus(device.deviceId, "revoked")}
              >
                Revoke
              </Button>
            ) : (
              <Button
                variant="outline"
                size="sm"
                disabled={busyDeviceId === device.deviceId}
                onClick={() => void setStatus(device.deviceId, "approved")}
              >
                Approve
              </Button>
            )}
          </div>
        </div>
      ))}
    </div>
  )
}

function AdminAccountsPanel() {
  const currentUserId = useVerificationStore((s) => s.verifiedUserId)
  const [accounts, setAccounts] = useState<AdminAccountRow[]>([])
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [busyUserId, setBusyUserId] = useState<string | null>(null)
  const [pendingDeleteId, setPendingDeleteId] = useState<string | null>(null)

  const [reloadCount, setReloadCount] = useState(0)

  useEffect(() => {
    let cancelled = false
    void adminListAccounts().then((result) => {
      if (cancelled) return
      if (result.ok) {
        setAccounts(result.accounts)
        setLoadError(null)
      } else {
        setLoadError(result.message)
      }
      setLoading(false)
    })
    return () => {
      cancelled = true
    }
  }, [reloadCount])

  const refresh = useCallback(async () => {
    setLoading(true)
    setLoadError(null)
    setReloadCount((count) => count + 1)
  }, [])

  async function handleSuspendToggle(account: AdminAccountRow) {
    setBusyUserId(account.user_id)
    const result = await adminSetSuspended(account.user_id, !account.suspended)
    setBusyUserId(null)
    if (result.ok) {
      toast.success(
        account.suspended
          ? `Reinstated ${account.email ?? account.user_id}`
          : `Suspended ${account.email ?? account.user_id}`
      )
      await refresh()
    } else {
      toast.error(result.message)
    }
  }

  async function handleExtendAccess(
    account: AdminAccountRow,
    option: (typeof ACCESS_EXTENSION_OPTIONS)[number]
  ) {
    setBusyUserId(account.user_id)
    const result = await adminSetAccess(account.user_id, option.days)
    setBusyUserId(null)
    if (result.ok) {
      toast.success(
        `Extended ${account.email ?? account.user_id} for ${option.toastLabel}`
      )
      await refresh()
    } else {
      toast.error(result.message)
    }
  }

  async function handleOfflineLeaseHours(
    account: AdminAccountRow,
    hours: 24 | 72 | 168
  ) {
    setBusyUserId(account.user_id)
    const result = await adminSetOfflineLeaseHours(account.user_id, hours)
    setBusyUserId(null)
    if (result.ok) {
      toast.success(`Offline allowance set to ${hours} hours.`)
      await refresh()
    } else {
      toast.error(result.message)
    }
  }

  async function handleDelete(account: AdminAccountRow) {
    setBusyUserId(account.user_id)
    const result = await adminDeleteAccount(account.user_id)
    setBusyUserId(null)
    setPendingDeleteId(null)
    if (result.ok) {
      toast.success(`Deleted ${account.email ?? account.user_id}`)
      await refresh()
    } else {
      toast.error(result.message)
    }
  }

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <ShieldIcon className="size-4 text-primary" />
          <p className="text-sm font-medium">Admin - Accounts</p>
        </div>
        <Button
          variant="outline"
          size="sm"
          disabled={loading}
          onClick={() => void refresh()}
        >
          <RefreshCwIcon className="mr-1.5 size-3.5" />
          Refresh
        </Button>
      </div>

      {loading ? (
        <p className="text-xs text-muted-foreground">Loading accounts...</p>
      ) : loadError ? (
        <p className="text-xs text-destructive">{loadError}</p>
      ) : accounts.length === 0 ? (
        <p className="text-xs text-muted-foreground">No accounts found.</p>
      ) : (
        <div className="space-y-2">
          {accounts.map((account) => {
            const isSelf = account.user_id === currentUserId
            const isBusy = busyUserId === account.user_id
            const confirmingDelete = pendingDeleteId === account.user_id
            return (
              <div key={account.user_id} className="glass-panel space-y-2 p-3">
                <div className="flex items-center justify-between gap-3">
                  <div className="min-w-0">
                    <p className="truncate text-sm font-medium">
                      {account.email ?? account.user_id}
                      {account.is_admin ? (
                        <span className="ml-2 rounded bg-primary/15 px-1.5 py-0.5 text-[10px] tracking-wide text-primary uppercase">
                          Admin
                        </span>
                      ) : null}
                      {account.is_church_organization ? (
                        <span className="ml-2 rounded bg-emerald-500/15 px-1.5 py-0.5 text-[10px] tracking-wide text-emerald-700 uppercase dark:text-emerald-300">
                          Church organization
                        </span>
                      ) : null}
                      {account.suspended ? (
                        <span className="ml-2 rounded bg-destructive/15 px-1.5 py-0.5 text-[10px] tracking-wide text-destructive uppercase">
                          Suspended
                        </span>
                      ) : null}
                    </p>
                    {account.is_church_organization && account.church_name ? (
                      <p className="truncate text-xs font-medium text-emerald-700 dark:text-emerald-300">
                        {account.church_name}
                      </p>
                    ) : null}
                    <p className="text-xs text-muted-foreground">
                      {account.device_count} device
                      {account.device_count === 1 ? "" : "s"} - last seen{" "}
                      {formatTimestamp(account.last_seen_at)}
                    </p>
                    <p className="text-xs text-muted-foreground">
                      Access{" "}
                      {formatAccessExpiry(
                        account.access_expires_at,
                        account.is_admin
                      )}
                    </p>
                  </div>
                  {account.is_admin ? null : confirmingDelete ? (
                    <div className="flex shrink-0 items-center gap-2">
                      <Button
                        variant="destructive"
                        size="sm"
                        disabled={isBusy}
                        onClick={() => void handleDelete(account)}
                      >
                        Confirm delete
                      </Button>
                      <Button
                        variant="ghost"
                        size="sm"
                        disabled={isBusy}
                        onClick={() => setPendingDeleteId(null)}
                      >
                        Cancel
                      </Button>
                    </div>
                  ) : (
                    <div className="flex shrink-0 flex-wrap items-center justify-end gap-2">
                      {ACCESS_EXTENSION_OPTIONS.map((option) => (
                        <Button
                          key={option.days}
                          variant="outline"
                          size="sm"
                          disabled={isBusy || isSelf}
                          onClick={() =>
                            void handleExtendAccess(account, option)
                          }
                        >
                          <CalendarPlusIcon className="mr-1.5 size-3.5" />
                          {option.label}
                        </Button>
                      ))}
                      <Button
                        variant="outline"
                        size="sm"
                        disabled={isBusy || isSelf}
                        onClick={() => void handleSuspendToggle(account)}
                      >
                        {account.suspended ? "Reinstate" : "Suspend"}
                      </Button>
                      <Button
                        variant="ghost"
                        size="sm"
                        disabled={isBusy || isSelf}
                        title={`Delete ${account.email ?? "account"}`}
                        aria-label={`Delete ${account.email ?? "account"}`}
                        onClick={() => setPendingDeleteId(account.user_id)}
                      >
                        <Trash2Icon className="size-3.5 text-destructive" />
                      </Button>
                    </div>
                  )}
                </div>
                <AdminDeviceManager userId={account.user_id} />
                <div className="flex flex-wrap items-center gap-2 border-t border-[var(--border-subtle)] pt-2">
                  <span className="text-xs text-muted-foreground">
                    Signed offline lease: {account.offline_lease_hours}h
                  </span>
                  {OFFLINE_LEASE_OPTIONS.map((option) => (
                    <Button
                      key={option.hours}
                      variant={
                        account.offline_lease_hours === option.hours
                          ? "default"
                          : "outline"
                      }
                      size="sm"
                      disabled={isBusy}
                      onClick={() =>
                        void handleOfflineLeaseHours(account, option.hours)
                      }
                    >
                      {option.label}
                    </Button>
                  ))}
                </div>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}

interface AccountDeletionPanelProps {
  isAdmin: boolean
  checkingAdmin: boolean
  confirmingDelete: boolean
  deleting: boolean
  onRequestDelete: () => void
  onCancelDelete: () => void
  onDelete: () => void
}

function AccountDeletionPanel({
  isAdmin,
  checkingAdmin,
  confirmingDelete,
  deleting,
  onRequestDelete,
  onCancelDelete,
  onDelete,
}: AccountDeletionPanelProps) {
  let action: ReactNode
  if (isAdmin) {
    action = (
      <p className="text-xs text-muted-foreground">
        Admin accounts are protected from self-delete. Remove the admin row in
        Supabase first if this account must be deleted.
      </p>
    )
  } else if (confirmingDelete) {
    action = (
      <div className="flex items-center gap-2">
        <Button
          variant="destructive"
          size="sm"
          disabled={deleting}
          onClick={onDelete}
        >
          {deleting ? "Deleting..." : "Yes, permanently delete"}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          disabled={deleting}
          onClick={onCancelDelete}
        >
          Cancel
        </Button>
      </div>
    )
  } else {
    action = (
      <Button
        variant="outline"
        size="sm"
        disabled={checkingAdmin}
        onClick={onRequestDelete}
      >
        <Trash2Icon className="mr-1.5 size-3.5 text-destructive" />
        {checkingAdmin ? "Checking account..." : "Delete account"}
      </Button>
    )
  }

  return (
    <div className="glass-panel space-y-3 p-4">
      <div>
        <p className="text-sm font-medium">Delete account</p>
        <p className="text-xs text-muted-foreground">
          Permanently removes your account and all registered devices. This
          cannot be undone.
        </p>
      </div>
      {action}
    </div>
  )
}

export function AccountSection() {
  const verifiedEmail = useVerificationStore((s) => s.verifiedEmail)
  const signOut = useVerificationStore((s) => s.signOut)
  const offlineGraceExpiresAt = useVerificationStore(
    (s) => s.offlineGraceExpiresAt
  )
  const [isAdmin, setIsAdmin] = useState(false)
  const [checkingAdmin, setCheckingAdmin] = useState(true)
  const [confirmingCancellation, setConfirmingCancellation] = useState(false)
  const [requestingCancellation, setRequestingCancellation] = useState(false)
  const [confirmingDelete, setConfirmingDelete] = useState(false)
  const [deleting, setDeleting] = useState(false)
  const [billingSummary, setBillingSummary] = useState<BillingSummary | null>(
    null
  )
  const [billingMessage, setBillingMessage] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    void fetchMyBillingSummary().then((result) => {
      if (cancelled) return
      if (result.ok) {
        setBillingSummary(result.summary)
        setBillingMessage(null)
      } else {
        setBillingMessage(result.message)
      }
    })
    return () => {
      cancelled = true
    }
  }, [])

  const subscriptionLabel = formatSubscriptionStatusLabel(billingSummary)
  const verifiedUserId = useVerificationStore((s) => s.verifiedUserId)

  async function reloadBillingSummary() {
    const result = await fetchMyBillingSummary()
    if (result.ok) {
      setBillingSummary(result.summary)
      setBillingMessage(null)
    }
  }

  useEffect(() => {
    let cancelled = false
    void fetchIsAdmin().then((result) => {
      if (cancelled) return
      setIsAdmin(result)
      setCheckingAdmin(false)
    })
    return () => {
      cancelled = true
    }
  }, [])

  async function handleDeleteAccount() {
    setDeleting(true)
    const result = await deleteOwnAccount()
    if (result.ok) {
      toast.success("Your account has been deleted.")
      await signOut()
    } else {
      setDeleting(false)
      setConfirmingDelete(false)
      toast.error(result.message)
    }
  }

  async function handleRequestCancellation() {
    setRequestingCancellation(true)
    const result = await requestAccountCancellation(verifiedEmail)
    try {
      await openSupportEmail(
        buildCancellationEmailOptions({ accountEmail: verifiedEmail })
      )
      toast.success(
        result.ok
          ? "Cancellation request saved. Email opened for your records."
          : "Cancellation email opened. Backend request could not be saved."
      )
      setConfirmingCancellation(false)
    } catch {
      toast.error(
        result.ok
          ? "Cancellation request saved. Could not open your email app."
          : "Could not save the request or open your email app. Contact support manually."
      )
    } finally {
      setRequestingCancellation(false)
    }
  }

  return (
    <div className="space-y-6">
      <div className="glass-panel flex items-center justify-between p-4">
        <div className="flex items-center gap-3">
          <div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-primary/10">
            <UserIcon className="size-4 text-primary" />
          </div>
          <div>
            <p className="text-sm font-medium">
              {verifiedEmail ?? "Signed in"}
            </p>
            <p className="text-xs text-muted-foreground">SabbathCue account</p>
            {offlineGraceExpiresAt ? (
              <p className="text-xs text-muted-foreground">
                Offline lease until{" "}
                {new Date(offlineGraceExpiresAt).toLocaleString()}
              </p>
            ) : null}
          </div>
        </div>
        <Button
          variant="outline"
          size="sm"
          disabled={deleting}
          onClick={() => void signOut()}
        >
          <LogOutIcon className="mr-1.5 size-3.5" />
          Sign out
        </Button>
      </div>

      <OwnDevicesPanel onCurrentDeactivated={signOut} />

      <div className="glass-panel space-y-3 p-4">
        <div>
          <p className="text-sm font-medium">Subscription</p>
          <p className="text-xs text-muted-foreground">
            Start or renew paid access with Paddle checkout. Manage billing,
            payment method, and cancellation in the customer portal.
          </p>
        </div>
        {billingMessage ? (
          <p className="text-xs text-muted-foreground">{billingMessage}</p>
        ) : subscriptionLabel ? (
          <p className="text-xs font-medium text-foreground">
            {subscriptionLabel}
          </p>
        ) : billingSummary?.paddleCustomerId ? (
          <p className="text-xs text-muted-foreground">
            No active subscription on file.
          </p>
        ) : null}
        {billingSummary?.accessExpiresAt ? (
          <p className="text-xs text-muted-foreground">
            App access until{" "}
            {new Date(billingSummary.accessExpiresAt).toLocaleString()}
          </p>
        ) : null}
        <div className="flex flex-wrap gap-2">
          {verifiedEmail && verifiedUserId ? (
            <PaddleSubscribePanel
              email={verifiedEmail}
              userId={verifiedUserId}
              onCompleted={() => void reloadBillingSummary()}
            />
          ) : null}
          {canActivateSubscriptionEarly(billingSummary) ? (
            <ActivateSubscriptionButton
              onActivated={() => void reloadBillingSummary()}
            />
          ) : null}
          {billingSummary?.paddleCustomerId ? (
            <ManageSubscriptionButton />
          ) : null}
        </div>
      </div>

      <div className="glass-panel space-y-3 p-4">
        <div>
          <p className="text-sm font-medium">Cancel subscription</p>
          <p className="text-xs text-muted-foreground">
            Stop renewal at the end of your current billing period or trial. You
            keep access until then; no refund for time already paid.
          </p>
        </div>
        <div className="rounded-lg border border-[var(--border-subtle)] bg-[var(--shell-bg-sunken)] p-3 text-xs text-muted-foreground">
          <p className="font-medium text-foreground">Cancellation disclaimer</p>
          <p className="mt-1">
            No refunds are issued for the current paid period. Your app access
            remains active until the subscribed period ends; after that,
            SabbathCue access is disabled unless renewed.
          </p>
        </div>
        {isSubscriptionCancelScheduled(billingSummary) ? (
          <p className="text-xs font-medium text-foreground">
            {subscriptionLabel ?? "Your subscription is scheduled to cancel."}
          </p>
        ) : canCancelSubscription(billingSummary) ? (
          <CancelSubscriptionButton
            onBillingStateChanged={() => void reloadBillingSummary()}
          />
        ) : confirmingCancellation ? (
          <div className="flex flex-wrap items-center gap-2">
            <Button
              variant="destructive"
              size="sm"
              disabled={requestingCancellation}
              onClick={() => void handleRequestCancellation()}
            >
              {requestingCancellation
                ? "Opening email..."
                : "Send cancellation request"}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              disabled={requestingCancellation}
              onClick={() => setConfirmingCancellation(false)}
            >
              Keep subscription
            </Button>
          </div>
        ) : (
          <Button
            variant="outline"
            size="sm"
            onClick={() => setConfirmingCancellation(true)}
          >
            <BanIcon className="mr-1.5 size-3.5" />
            Request cancellation by email
          </Button>
        )}
      </div>

      <AccountDeletionPanel
        isAdmin={isAdmin}
        checkingAdmin={checkingAdmin}
        confirmingDelete={confirmingDelete}
        deleting={deleting}
        onRequestDelete={() => setConfirmingDelete(true)}
        onCancelDelete={() => setConfirmingDelete(false)}
        onDelete={() => void handleDeleteAccount()}
      />

      {isAdmin ? (
        <>
          <AdminAccountsPanel />
          <AnnouncementsAdminPanel />
        </>
      ) : null}
    </div>
  )
}
