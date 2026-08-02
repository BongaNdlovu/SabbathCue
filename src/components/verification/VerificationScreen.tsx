import { useState, type ComponentProps, type FormEvent } from "react"
import {
  ArrowLeftIcon,
  CheckCircle2Icon,
  KeyRoundIcon,
  LoaderCircleIcon,
  LogInIcon,
  MailIcon,
  ShieldAlertIcon,
  ShieldCheckIcon,
  UserPlusIcon,
} from "lucide-react"
import { AppLogo } from "@/components/ui/app-logo"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { SegmentedControl } from "@/components/ui/segmented-control"
import { APP_DISPLAY_NAME } from "@/lib/app-brand"
import { cn } from "@/lib/utils"
import { requestPasswordReset } from "@/lib/supabase/auth"
import { PaddleSubscribeButton } from "@/components/billing/PaddleSubscribeButton"
import { EftPaymentOptions } from "@/components/billing/EftPaymentOptions"
import { isPaddleCheckoutConfigured } from "@/lib/paddle/config"
import {
  accentThemeClassName,
  useAccentThemeStore,
} from "@/stores/accent-theme-store"
import {
  darkSurfaceClassName,
  useColorModeStore,
} from "@/stores/color-mode-store"
import { useVerificationStore } from "@/stores/verification-store"
import type {
  VerificationErrorCode,
  VerificationStatus,
} from "@/types/verification"

type AuthMode = "sign-in" | "create-account"
type NoticeTone = "success" | "error" | "info"
type PendingAction = AuthMode | "reset-password" | null

const MIN_PASSWORD_LENGTH = 6

const AUTH_MODE_OPTIONS = [
  { value: "sign-in", label: "Sign in" },
  { value: "create-account", label: "Start trial" },
] satisfies Array<{ value: AuthMode; label: string }>

function formatErrorMessage(
  errorCode: VerificationErrorCode | null,
  error: string | null
): string {
  switch (errorCode) {
    case "invalid_credentials":
      return "Email or password is incorrect."
    case "email_not_confirmed":
      return "Confirm your email address before signing in."
    case "device_limit_reached":
      return (
        error ??
        "This account is already registered on the maximum number of devices."
      )
    case "device_pending":
      return "This computer is waiting for activation approval. Ask an administrator to approve it."
    case "device_revoked":
      return "This computer has been deactivated. Reactivate it from an approved computer or contact an administrator."
    case "suspended":
      return (
        error ??
        "This account has been suspended. Contact support for assistance."
      )
    case "trial_expired":
      return error ?? "Your access has ended. Contact the developer to renew."
    case "network":
      return "Unable to connect. Check your network and try again."
    default:
      return error ?? "Sign in failed. Try again when ready."
  }
}

function isValidEmail(email: string): boolean {
  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)
}

function validateEmail(email: string): string | null {
  if (!email.trim()) return "Enter your email address."
  if (!isValidEmail(email.trim())) return "Enter a valid email address."
  return null
}

function validatePassword(password: string): string | null {
  if (!password) return "Enter your password."
  if (password.length < MIN_PASSWORD_LENGTH) {
    return `Password must be at least ${MIN_PASSWORD_LENGTH} characters.`
  }
  return null
}

function modeTitle(mode: AuthMode, status: VerificationStatus): string {
  if (status === "expired") return "Session expired"
  if (status === "checking") return "Checking your account"
  return mode === "create-account" ? "Start your free trial" : "Sign in"
}

function modeDescription(mode: AuthMode, status: VerificationStatus): string {
  if (status === "expired") {
    return "Your saved session is no longer valid. Sign in again or clear it."
  }
  if (status === "checking") {
    return "This should only take a moment."
  }
  if (mode === "create-account") {
    return "Create an account to start 14 days of SabbathCue access."
  }
  return "Continue with your SabbathCue account."
}

function AuthNotice({
  tone,
  children,
}: {
  tone: NoticeTone
  children: string
}) {
  const Icon =
    tone === "success"
      ? CheckCircle2Icon
      : tone === "error"
        ? ShieldAlertIcon
        : ShieldCheckIcon

  return (
    <div
      className={cn(
        "flex gap-2 rounded-md border px-3 py-2 text-left text-xs leading-relaxed",
        tone === "success" &&
          "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-200",
        tone === "error" &&
          "border-red-500/30 bg-red-500/10 text-red-700 dark:text-red-200",
        tone === "info" &&
          "border-sky-500/30 bg-sky-500/10 text-sky-700 dark:text-sky-200"
      )}
      role={tone === "error" ? "alert" : "status"}
    >
      <Icon className="mt-0.5 size-4 shrink-0" />
      <span>{children}</span>
    </div>
  )
}

function Field({
  label,
  help,
  ...props
}: ComponentProps<typeof Input> & {
  label: string
  help?: string
}) {
  return (
    <label className="flex flex-col gap-1.5 text-left">
      <span className="font-mono text-[10px] font-semibold tracking-wider text-muted-foreground uppercase">
        {label}
      </span>
      <Input {...props} />
      {help ? (
        <span className="text-[11px] leading-relaxed font-normal text-muted-foreground">
          {help}
        </span>
      ) : null}
    </label>
  )
}

function PasswordResetForm({
  initialEmail,
  busy,
  onBusyChange,
  onBack,
}: {
  initialEmail: string
  busy: boolean
  onBusyChange: (busy: boolean) => void
  onBack: (notice?: string) => void
}) {
  const [email, setEmail] = useState(initialEmail)
  const [sentEmail, setSentEmail] = useState<string | null>(null)
  const [message, setMessage] = useState<string | null>(null)
  const [tone, setTone] = useState<NoticeTone>("info")

  async function handleSendLink(event?: FormEvent<HTMLFormElement>) {
    event?.preventDefault()

    const nextEmail = email.trim()
    const emailError = validateEmail(nextEmail)
    if (emailError) {
      setTone("error")
      setMessage(emailError)
      return
    }

    onBusyChange(true)
    setMessage(null)
    const result = await requestPasswordReset(nextEmail)
    onBusyChange(false)

    if (!result.ok) {
      setTone("error")
      setMessage(result.message)
      return
    }

    setSentEmail(nextEmail)
    setTone("success")
    setMessage(
      `Reset link sent to ${nextEmail}. Open the latest email and choose a new password.`
    )
  }

  return (
    <form
      className="flex flex-col gap-4"
      onSubmit={(event) => void handleSendLink(event)}
    >
      <button
        type="button"
        className="btn-action inline-flex h-7 w-fit items-center gap-1 rounded-md px-1.5 text-xs font-medium text-muted-foreground hover:bg-[var(--shell-bg-sunken)] hover:text-foreground"
        disabled={busy}
        onClick={() =>
          onBack(
            sentEmail
              ? `Reset link sent to ${sentEmail}. After updating it in your browser, sign in with the new password.`
              : undefined
          )
        }
      >
        <ArrowLeftIcon className="size-3.5" />
        Sign in
      </button>

      <div className="space-y-1 text-left">
        <div className="flex items-center gap-2 text-base font-semibold text-foreground">
          <KeyRoundIcon className="size-5 text-[var(--brand-accent)]" />
          Reset password
        </div>
        <p className="text-sm leading-relaxed text-muted-foreground">
          Send a recovery link to the email on your SabbathCue account.
        </p>
      </div>

      {message ? <AuthNotice tone={tone}>{message}</AuthNotice> : null}

      <Field
        autoComplete="email"
        disabled={busy}
        label="Email"
        placeholder="you@example.com"
        type="email"
        value={email}
        onChange={(event) => setEmail(event.target.value)}
      />

      <Button className="w-full" disabled={busy} type="submit">
        {busy ? (
          <>
            <LoaderCircleIcon className="size-4 animate-spin" />
            Sending
          </>
        ) : (
          <>
            <MailIcon className="size-4" />
            {sentEmail ? "Resend reset link" : "Send reset link"}
          </>
        )}
      </Button>
    </form>
  )
}

function authFeedback({
  status,
  error,
  errorCode,
  localMessage,
  localTone,
}: {
  status: VerificationStatus
  error: string | null
  errorCode: VerificationErrorCode | null
  localMessage: string | null
  localTone: NoticeTone
}) {
  const staleSession = status === "expired"
  const storeNotice = status === "required" && error ? error : null
  const storeError =
    status === "error" ? formatErrorMessage(errorCode, error) : null

  // device_pending is retryable because the saved session is still valid: once
  // an administrator approves this named computer, the same signed
  // registration call succeeds. Every other device/account block needs an
  // explicit administrative decision first, so it gets no Retry.
  const retryableError =
    status === "error" &&
    (errorCode === "network" || errorCode === "device_pending")

  return {
    canRetry: retryableError || staleSession,
    message: localMessage ?? storeError ?? storeNotice,
    staleSession,
    tone:
      localMessage !== null
        ? localTone
        : storeError
          ? ("error" as const)
          : ("success" as const),
  }
}

function submitLabelFor(mode: AuthMode, pendingAction: PendingAction): string {
  if (mode === "create-account") {
    return pendingAction === "create-account"
      ? "Starting trial"
      : "Start 14-day trial"
  }
  return pendingAction === "sign-in" ? "Signing in" : "Sign in"
}

function AuthFields({
  mode,
  busy,
  email,
  password,
  confirmPassword,
  isChurchOrganization,
  churchName,
  onEmailChange,
  onPasswordChange,
  onConfirmPasswordChange,
  onChurchOrganizationChange,
  onChurchNameChange,
}: {
  mode: AuthMode
  busy: boolean
  email: string
  password: string
  confirmPassword: string
  isChurchOrganization: boolean
  churchName: string
  onEmailChange: (email: string) => void
  onPasswordChange: (password: string) => void
  onConfirmPasswordChange: (password: string) => void
  onChurchOrganizationChange: (selected: boolean) => void
  onChurchNameChange: (name: string) => void
}) {
  return (
    <>
      <Field
        autoComplete="email"
        disabled={busy}
        label="Email"
        placeholder="you@example.com"
        type="email"
        value={email}
        onChange={(event) => onEmailChange(event.target.value)}
      />
      <Field
        autoComplete={
          mode === "create-account" ? "new-password" : "current-password"
        }
        disabled={busy}
        help={
          mode === "create-account"
            ? `Use at least ${MIN_PASSWORD_LENGTH} characters.`
            : undefined
        }
        label="Password"
        placeholder="Password"
        type="password"
        value={password}
        onChange={(event) => onPasswordChange(event.target.value)}
      />
      {mode === "create-account" ? (
        <>
          <Field
            autoComplete="new-password"
            disabled={busy}
            label="Confirm password"
            placeholder="Confirm password"
            type="password"
            value={confirmPassword}
            onChange={(event) => onConfirmPasswordChange(event.target.value)}
          />
          <label className="flex items-start gap-2 rounded-md border border-[var(--border-subtle)] bg-[var(--shell-bg-sunken)] px-3 py-2 text-left">
            <input
              checked={isChurchOrganization}
              className="mt-0.5 size-4 accent-[var(--brand-accent)]"
              disabled={busy}
              type="checkbox"
              onChange={(event) =>
                onChurchOrganizationChange(event.target.checked)
              }
            />
            <span>
              <span className="block text-sm font-medium text-foreground">
                We are a church organization
              </span>
              <span className="block text-[11px] leading-relaxed text-muted-foreground">
                Adds a self-declared church badge to this account.
              </span>
            </span>
          </label>
          {isChurchOrganization ? (
            <Field
              autoComplete="organization"
              disabled={busy}
              help="Required for church organization accounts."
              label="Church name"
              maxLength={120}
              placeholder="Example Seventh-day Adventist Church"
              value={churchName}
              onChange={(event) => onChurchNameChange(event.target.value)}
            />
          ) : null}
        </>
      ) : null}
    </>
  )
}

function AuthFooterActions({
  busy,
  canRetry,
  staleSession,
  onForgotPassword,
  onRetry,
  onClearSession,
}: {
  busy: boolean
  canRetry: boolean
  staleSession: boolean
  onForgotPassword: () => void
  onRetry: () => void
  onClearSession: () => void
}) {
  return (
    <div className="flex flex-wrap items-center justify-center gap-2 border-t border-[var(--border-dim)] pt-4">
      <Button
        disabled={busy}
        size="sm"
        type="button"
        variant="ghost"
        onClick={onForgotPassword}
      >
        <KeyRoundIcon className="size-4" />
        Forgot password?
      </Button>

      {canRetry ? (
        <Button
          disabled={busy}
          size="sm"
          type="button"
          variant="ghost"
          onClick={onRetry}
        >
          <LoaderCircleIcon className="size-4" />
          Retry
        </Button>
      ) : null}

      {staleSession ? (
        <Button
          disabled={busy}
          size="sm"
          type="button"
          variant="ghost"
          onClick={onClearSession}
        >
          <ArrowLeftIcon className="size-4" />
          Clear session
        </Button>
      ) : null}
    </div>
  )
}

function TrialExpiredPanel({
  busy,
  message,
  accountEmail,
  accountUserId,
  onRetry,
  onSignOut,
  onCheckoutCompleted,
}: {
  busy: boolean
  message: string
  accountEmail: string | null
  accountUserId: string | null
  onRetry: () => void
  onSignOut: () => void
  onCheckoutCompleted: () => void
}) {
  const paddleReady =
    isPaddleCheckoutConfigured() &&
    Boolean(accountEmail?.trim()) &&
    Boolean(accountUserId)

  return (
    <div className="flex flex-col gap-5 text-left">
      <div className="flex items-start gap-3">
        <div className="flex size-10 shrink-0 items-center justify-center rounded-lg bg-amber-500/12 text-amber-700 dark:text-amber-300">
          <ShieldAlertIcon className="size-5" />
        </div>
        <div className="space-y-1">
          <h1 className="text-xl font-semibold text-foreground">
            Access ended
          </h1>
          <p className="text-sm leading-relaxed text-muted-foreground">
            {message}
          </p>
        </div>
      </div>

      {paddleReady && accountEmail && accountUserId ? (
        <div className="space-y-2">
          <PaddleSubscribeButton
            email={accountEmail}
            userId={accountUserId}
            label="Subscribe with Paddle"
            disabled={busy}
            size="default"
            onCompleted={onCheckoutCompleted}
          />
          <p className="text-xs text-muted-foreground">
            Card checkout via Paddle. After payment, choose Retry to refresh
            your access.
          </p>
        </div>
      ) : null}

      {paddleReady ? (
        <div className="relative py-1">
          <div className="absolute inset-x-0 top-1/2 h-px bg-[var(--border-dim)]" />
          <p className="relative mx-auto w-fit bg-[var(--bg-surface)] px-3 text-[10px] font-semibold tracking-wider text-muted-foreground uppercase">
            or
          </p>
        </div>
      ) : null}

      <EftPaymentOptions accountEmail={accountEmail} disabled={busy} />

      <div className="flex flex-wrap items-center justify-center gap-2 border-t border-[var(--border-dim)] pt-4">
        <Button
          disabled={busy}
          size="sm"
          type="button"
          variant="ghost"
          onClick={onRetry}
        >
          <LoaderCircleIcon className="size-4" />
          Retry
        </Button>
        <Button
          disabled={busy}
          size="sm"
          type="button"
          variant="ghost"
          onClick={onSignOut}
        >
          <ArrowLeftIcon className="size-4" />
          Sign out
        </Button>
      </div>
    </div>
  )
}

function VerificationStatusChip({ status }: { status: VerificationStatus }) {
  const label =
    status === "checking"
      ? "Checking"
      : status === "expired"
        ? "Session expired"
        : status === "error"
          ? "Attention needed"
          : "Account required"

  return (
    <span
      className={cn(
        "inline-flex h-7 items-center gap-1.5 rounded-md border px-2 font-mono text-[10px] font-semibold tracking-wide uppercase",
        status === "expired" || status === "error"
          ? "border-amber-500/35 bg-amber-500/12 text-amber-700 dark:text-amber-300"
          : "border-[var(--border-subtle)] bg-[var(--shell-bg-sunken)] text-muted-foreground"
      )}
    >
      <ShieldCheckIcon className="size-3" />
      {label}
    </span>
  )
}

function VerificationHeader({ status }: { status: VerificationStatus }) {
  return (
    <header className="z-50 flex h-[58px] shrink-0 items-center justify-between border-b border-[var(--border-subtle)] bg-[color-mix(in_srgb,var(--shell-bg-sunken)_86%,transparent)] px-5 backdrop-blur-xl">
      <div className="flex items-center gap-3">
        <AppLogo
          size="sm"
          className="transition-transform duration-300 hover:rotate-3"
        />
        <div className="flex flex-col leading-none">
          <span className="font-display text-xl tracking-wide text-foreground">
            {APP_DISPLAY_NAME}
          </span>
          <span className="mt-0.5 font-mono text-[9px] tracking-wider text-muted-foreground uppercase">
            Account Access
          </span>
        </div>
      </div>

      <VerificationStatusChip status={status} />
    </header>
  )
}

function VerificationSidePanel() {
  return (
    <section
      aria-label={`${APP_DISPLAY_NAME} account`}
      className="hidden border-r border-[var(--border-subtle)] bg-[var(--shell-bg-sunken)] p-6 md:flex md:flex-col md:justify-between"
    >
      <div className="space-y-5">
        <div className="space-y-2">
          <span className="font-mono text-[10px] font-semibold tracking-wider text-[var(--accent)] uppercase">
            Operator account
          </span>
          <h2 className="max-w-xs text-2xl leading-tight font-semibold text-foreground">
            Sign in to open the presentation controller.
          </h2>
          <p className="max-w-sm text-sm leading-relaxed text-muted-foreground">
            Your account keeps this installation connected to the verified
            SabbathCue workspace.
          </p>
        </div>

        <div className="grid gap-2 border-y border-[var(--border-subtle)] py-4">
          {["Controller shell", "Broadcast outputs", "Library sync"].map(
            (item) => (
              <div
                key={item}
                className="flex items-center justify-between gap-3 text-xs"
              >
                <span className="text-muted-foreground">{item}</span>
                <span className="rounded-md border border-[var(--border-subtle)] bg-[var(--shell-bg-sunken)] px-2 py-0.5 font-mono text-[10px] font-semibold text-foreground uppercase">
                  Ready
                </span>
              </div>
            )
          )}
        </div>
      </div>

      <div className="flex items-center gap-2 rounded-md border border-[var(--border-subtle)] bg-[var(--shell-bg-sunken)] px-2 py-1 font-mono text-[10px]">
        <span className="size-1.5 rounded-full bg-[var(--accent)]" />
        <span className="text-muted-foreground">Secure session</span>
      </div>
    </section>
  )
}

function AuthPanel({
  mode,
  status,
  busy,
  pendingAction,
  email,
  password,
  confirmPassword,
  isChurchOrganization,
  churchName,
  message,
  tone,
  canRetry,
  staleSession,
  onModeChange,
  onEmailChange,
  onPasswordChange,
  onConfirmPasswordChange,
  onChurchOrganizationChange,
  onChurchNameChange,
  onSignIn,
  onSignUp,
  onForgotPassword,
  onRetry,
  onClearSession,
}: {
  mode: AuthMode
  status: VerificationStatus
  busy: boolean
  pendingAction: PendingAction
  email: string
  password: string
  confirmPassword: string
  isChurchOrganization: boolean
  churchName: string
  message: string | null
  tone: NoticeTone
  canRetry: boolean
  staleSession: boolean
  onModeChange: (mode: AuthMode) => void
  onEmailChange: (email: string) => void
  onPasswordChange: (password: string) => void
  onConfirmPasswordChange: (password: string) => void
  onChurchOrganizationChange: (selected: boolean) => void
  onChurchNameChange: (name: string) => void
  onSignIn: (event: FormEvent<HTMLFormElement>) => void
  onSignUp: (event: FormEvent<HTMLFormElement>) => void
  onForgotPassword: () => void
  onRetry: () => void
  onClearSession: () => void
}) {
  const SubmitIcon = mode === "create-account" ? UserPlusIcon : LogInIcon

  return (
    <>
      <div className="space-y-4">
        <SegmentedControl
          aria-label="Authentication mode"
          className="w-full justify-center"
          options={AUTH_MODE_OPTIONS}
          value={mode}
          onChange={onModeChange}
        />

        <div className="space-y-1 text-left">
          <h1 className="text-xl font-semibold text-foreground">
            {modeTitle(mode, status)}
          </h1>
          <p className="text-sm leading-relaxed text-muted-foreground">
            {modeDescription(mode, status)}
          </p>
        </div>
      </div>

      {message ? <AuthNotice tone={tone}>{message}</AuthNotice> : null}

      <form
        className="flex flex-col gap-4"
        onSubmit={(event) =>
          mode === "create-account" ? onSignUp(event) : onSignIn(event)
        }
      >
        <AuthFields
          busy={busy}
          confirmPassword={confirmPassword}
          isChurchOrganization={isChurchOrganization}
          churchName={churchName}
          email={email}
          mode={mode}
          password={password}
          onConfirmPasswordChange={onConfirmPasswordChange}
          onChurchOrganizationChange={onChurchOrganizationChange}
          onChurchNameChange={onChurchNameChange}
          onEmailChange={onEmailChange}
          onPasswordChange={onPasswordChange}
        />

        <Button className="w-full" disabled={busy} type="submit">
          {pendingAction === mode ? (
            <LoaderCircleIcon className="size-4 animate-spin" />
          ) : (
            <SubmitIcon className="size-4" />
          )}
          {submitLabelFor(mode, pendingAction)}
        </Button>
      </form>

      <AuthFooterActions
        busy={busy}
        canRetry={canRetry}
        staleSession={staleSession}
        onClearSession={onClearSession}
        onForgotPassword={onForgotPassword}
        onRetry={onRetry}
      />
    </>
  )
}

export function VerificationScreen() {
  const accentTheme = useAccentThemeStore((s) => s.theme)
  const darkSurface = useColorModeStore((s) => s.darkSurface)
  const status = useVerificationStore((s) => s.status)
  const error = useVerificationStore((s) => s.error)
  const errorCode = useVerificationStore((s) => s.errorCode)
  const verifiedEmail = useVerificationStore((s) => s.verifiedEmail)
  const verifiedUserId = useVerificationStore((s) => s.verifiedUserId)
  const signIn = useVerificationStore((s) => s.signIn)
  const signUp = useVerificationStore((s) => s.signUp)
  const signOut = useVerificationStore((s) => s.signOut)
  const refresh = useVerificationStore((s) => s.refresh)

  const [mode, setMode] = useState<AuthMode>("sign-in")
  const [showReset, setShowReset] = useState(false)
  const [email, setEmail] = useState("")
  const [password, setPassword] = useState("")
  const [confirmPassword, setConfirmPassword] = useState("")
  const [isChurchOrganization, setIsChurchOrganization] = useState(false)
  const [churchName, setChurchName] = useState("")
  const [localMessage, setLocalMessage] = useState<string | null>(null)
  const [localTone, setLocalTone] = useState<NoticeTone>("info")
  const [pendingAction, setPendingAction] = useState<PendingAction>(null)

  const isChecking = status === "checking"
  const isBusy = isChecking || pendingAction !== null
  const feedback = authFeedback({
    error,
    errorCode,
    localMessage,
    localTone,
    status,
  })
  const isTrialExpired = status === "error" && errorCode === "trial_expired"

  function setModeAndClear(nextMode: AuthMode) {
    setMode(nextMode)
    setShowReset(false)
    setLocalMessage(null)
    setLocalTone("info")
    setConfirmPassword("")
    setIsChurchOrganization(false)
    setChurchName("")
  }

  function validateCredentials(
    nextEmail: string,
    nextPassword: string
  ): string | null {
    return validateEmail(nextEmail) ?? validatePassword(nextPassword)
  }

  async function handleSignIn(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const nextEmail = email.trim()
    const validationError = validateCredentials(nextEmail, password)
    if (validationError) {
      setLocalTone("error")
      setLocalMessage(validationError)
      return
    }

    setLocalMessage(null)
    setPendingAction("sign-in")
    await signIn(nextEmail, password)
    setPendingAction(null)
  }

  async function handleSignUp(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const nextEmail = email.trim()
    const validationError = validateCredentials(nextEmail, password)
    if (validationError) {
      setLocalTone("error")
      setLocalMessage(validationError)
      return
    }
    if (password !== confirmPassword) {
      setLocalTone("error")
      setLocalMessage("Passwords do not match.")
      return
    }
    const normalizedChurchName = churchName.trim()
    if (
      isChurchOrganization &&
      (normalizedChurchName.length < 2 || normalizedChurchName.length > 120)
    ) {
      setLocalTone("error")
      setLocalMessage("Enter a church name between 2 and 120 characters.")
      return
    }

    setLocalMessage(null)
    setPendingAction("create-account")
    await signUp(nextEmail, password, {
      isChurchOrganization,
      churchName: isChurchOrganization ? normalizedChurchName : null,
    })

    const nextState = useVerificationStore.getState()
    if (nextState.status === "required" && nextState.error) {
      setMode("sign-in")
      setPassword("")
      setConfirmPassword("")
      setLocalTone("success")
      setLocalMessage(nextState.error)
    }
    setPendingAction(null)
  }

  return (
    <div
      id="bodyThemeContainer"
      className={cn(
        accentThemeClassName(accentTheme),
        darkSurfaceClassName(darkSurface),
        "fixed inset-0 overflow-hidden bg-[var(--bg-deep)] text-foreground"
      )}
    >
      <div className="app-shell">
        <VerificationHeader status={status} />

        <main className="relative z-10 flex flex-1 items-center justify-center overflow-y-auto p-4">
          <div className="glass-panel grid w-full max-w-4xl md:min-h-[520px] md:grid-cols-[0.95fr_1.05fr]">
            <VerificationSidePanel />

            <section className="flex items-center p-5 sm:p-8">
              <div className="mx-auto flex w-full max-w-sm flex-col gap-5">
                <div className="flex items-center justify-between gap-3 md:hidden">
                  <span className="font-mono text-[10px] font-semibold tracking-wider text-[var(--accent)] uppercase">
                    Operator account
                  </span>
                  <ShieldCheckIcon className="size-5 text-[var(--brand-accent)]" />
                </div>

                {isTrialExpired ? (
                  <TrialExpiredPanel
                    busy={isBusy}
                    message={
                      feedback.message ??
                      "Your access has ended. Subscribe to renew, or contact support."
                    }
                    accountEmail={verifiedEmail ?? null}
                    accountUserId={verifiedUserId ?? null}
                    onCheckoutCompleted={() => void refresh()}
                    onRetry={() => void refresh()}
                    onSignOut={() => void signOut()}
                  />
                ) : showReset ? (
                  <PasswordResetForm
                    busy={pendingAction === "reset-password"}
                    initialEmail={email.trim()}
                    onBusyChange={(busy) =>
                      setPendingAction(busy ? "reset-password" : null)
                    }
                    onBack={(notice) => {
                      setShowReset(false)
                      if (notice) {
                        setLocalTone("success")
                        setLocalMessage(notice)
                      }
                    }}
                  />
                ) : (
                  <AuthPanel
                    busy={isBusy}
                    canRetry={feedback.canRetry}
                    confirmPassword={confirmPassword}
                    isChurchOrganization={isChurchOrganization}
                    churchName={churchName}
                    email={email}
                    message={feedback.message}
                    mode={mode}
                    password={password}
                    pendingAction={pendingAction}
                    staleSession={feedback.staleSession}
                    status={status}
                    tone={feedback.tone}
                    onClearSession={() => void signOut()}
                    onConfirmPasswordChange={setConfirmPassword}
                    onChurchOrganizationChange={(selected) => {
                      setIsChurchOrganization(selected)
                      if (!selected) setChurchName("")
                    }}
                    onChurchNameChange={setChurchName}
                    onEmailChange={setEmail}
                    onForgotPassword={() => {
                      setLocalMessage(null)
                      setShowReset(true)
                    }}
                    onModeChange={setModeAndClear}
                    onPasswordChange={setPassword}
                    onRetry={() => void refresh()}
                    onSignIn={(event) => void handleSignIn(event)}
                    onSignUp={(event) => void handleSignUp(event)}
                  />
                )}
              </div>
            </section>
          </div>
        </main>
      </div>
    </div>
  )
}
