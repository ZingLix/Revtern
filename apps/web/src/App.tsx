import { ApiError, type CatalogConfirmation } from "@revtern/api-client";
import type {
  AppMemberRecord,
  AppRecord,
  AppRoleRecord,
  AppStoreTestEnvironment,
  AppStoreTestNotification,
  DataSourceRecord,
  LogicalProductRecord,
  RawEventRecord,
  SubscriptionRecord,
  TransactionRecord,
} from "@revtern/types";
import {
  Alert,
  Button,
  Card,
  Checkbox,
  Chip,
  Drawer,
  EmptyState as HeroEmptyState,
  Input,
  ListBox,
  Link,
  Select,
  Table,
  TextArea,
  ToggleButton,
  ToggleButtonGroup,
  Toolbar,
  Typography,
  useOverlayState,
} from "@heroui/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  AlertTriangle,
  ArrowUpRight,
  Boxes,
  Check,
  CloudDownload,
  Copy,
  Database,
  KeyRound,
  LayoutDashboard,
  LineChartIcon,
  ListChecks,
  LogOut,
  Minus,
  Plus,
  RadioTower,
  Receipt,
  RefreshCw,
  Search,
  Send,
  Settings,
  ShieldCheck,
  SquareStack,
  TrendingUp,
  Trash2,
  UserPlus,
  Users,
  X,
} from "lucide-react";
import { createContext, FormEvent, ReactNode, useContext, useEffect, useMemo, useState } from "react";
import { NavLink as RouterNavLink, Navigate, Route, Routes, useLocation, useNavigate } from "react-router-dom";
import {
  Bar,
  BarChart,
  CartesianGrid,
  Legend,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { API_BASE_URL, api } from "./api";
import { buildCatalogDraft, type CatalogDraftGroup } from "./lib/catalog";
import { formatCompactMoney, formatDate, formatDateTime, formatMoney, formatNumber, formatPercent, last30Days, titleize } from "./lib/format";

const navItems = [
  { to: "/", label: "Overview", icon: LayoutDashboard, permission: "app.read" },
  { to: "/revenue", label: "Revenue", icon: LineChartIcon, permission: "app.read" },
  { to: "/transactions", label: "Transactions", icon: Receipt, permission: "ledger.read" },
  { to: "/subscriptions", label: "Subscriptions", icon: SquareStack, permission: "ledger.read" },
  { to: "/products", label: "Products", icon: Boxes, permission: "app.read" },
  { to: "/events", label: "Events", icon: Database, permission: "events.sensitive.read" },
  { to: "/sources", label: "Sources", icon: RadioTower, permission: "app.read" },
  { to: "/reconciliation", label: "Reconciliation", icon: AlertTriangle, permission: "app.read" },
  { to: "/jobs", label: "Jobs", icon: ListChecks, permission: "jobs.run" },
  { to: "/settings", label: "Settings", icon: Settings, permission: null },
];

export default function App() {
  const location = useLocation();
  const invitationToken = invitationTokenFromPath(location.pathname);
  const setup = useQuery({ queryKey: ["setup-status"], queryFn: () => api.setupStatus() });
  const me = useQuery({
    queryKey: ["me"],
    queryFn: () => api.me(),
    retry: false,
    enabled: setup.data ? !setup.data.needs_setup : false,
  });

  if (setup.isLoading) return <BootScreen />;
  if (setup.error) return <AuthFrame><ErrorBlock error={setup.error} /></AuthFrame>;
  if (setup.data?.needs_setup) return <SetupScreen />;
  if (me.isLoading) return <BootScreen />;
  if (invitationToken) {
    return <InvitationScreen authenticated={Boolean(me.data)} token={invitationToken} />;
  }
  if (me.error) {
    return (
      <LoginScreen
        authMode={setup.data?.auth_mode ?? "local"}
        oidcName={setup.data?.oidc?.name}
        registrationMode={setup.data?.registration_mode ?? "invite_only"}
      />
    );
  }
  if (!me.data) return <BootScreen />;
  return <AppShell me={me.data} />;
}

function BootScreen() {
  return (
    <div className="boot-screen">
      <div className="boot-mark">R</div>
      <span>Loading Revtern</span>
    </div>
  );
}

function AuthFrame({ children }: { children: ReactNode }) {
  return (
    <main className="auth-page">
      <Card className="auth-panel" variant="default">
        <Card.Content className="auth-panel__content">
          <div className="brand-row">
            <span className="brand-mark">R</span>
            <strong>Revtern</strong>
          </div>
          {children}
        </Card.Content>
      </Card>
    </main>
  );
}

function SetupScreen() {
  const queryClient = useQueryClient();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [workspaceName, setWorkspaceName] = useState("Personal Apps");
  const mutation = useMutation({
    mutationFn: () => api.setupOwner({ email, password, workspace_name: workspaceName }),
    onSuccess: async () => {
      await queryClient.invalidateQueries();
    },
  });

  return (
    <AuthFrame>
      <form
        className="stack"
        onSubmit={(event) => {
          event.preventDefault();
          mutation.mutate();
        }}
      >
        <h1>First-run setup</h1>
        <Field label="Owner email">
          <Input fullWidth value={email} onChange={(event) => setEmail(event.target.value)} type="email" autoFocus required variant="secondary" />
        </Field>
        <Field label="Password">
          <Input fullWidth value={password} onChange={(event) => setPassword(event.target.value)} type="password" minLength={8} required variant="secondary" />
        </Field>
        <Field label="Workspace">
          <Input fullWidth value={workspaceName} onChange={(event) => setWorkspaceName(event.target.value)} required variant="secondary" />
        </Field>
        {mutation.error ? <ErrorBlock error={mutation.error} /> : null}
        <Button isDisabled={mutation.isPending} size="sm" type="submit" variant="primary">
          <Check size={16} />
          Create owner
        </Button>
      </form>
    </AuthFrame>
  );
}

function LoginScreen({
  authMode,
  oidcName,
  registrationMode,
}: {
  authMode: string;
  oidcName?: string;
  registrationMode: "closed" | "invite_only" | "open";
}) {
  const queryClient = useQueryClient();
  const [mode, setMode] = useState<"login" | "register">("login");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [displayName, setDisplayName] = useState("");
  const mutation = useMutation({
    mutationFn: async () => {
      if (mode === "login") {
        await api.login({ email, password });
      } else {
        await api.register({ email, password, display_name: displayName || undefined });
      }
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries();
    },
  });

  return (
    <AuthFrame>
      <form
        className="stack"
        onSubmit={(event) => {
          event.preventDefault();
          mutation.mutate();
        }}
      >
        <h1>{mode === "login" ? "Sign in" : "Create account"}</h1>
        {registrationMode === "open" ? (
          <Segmented value={mode} onChange={(value) => setMode(value as "login" | "register")} options={["login", "register"]} />
        ) : null}
        {authMode === "reverse_proxy" ? <p className="muted">Reverse proxy mode is enabled; the trusted user header was not present.</p> : null}
        {mode === "register" ? (
          <Field label="Display name">
            <Input fullWidth value={displayName} onChange={(event) => setDisplayName(event.target.value)} variant="secondary" />
          </Field>
        ) : null}
        <Field label="Email">
          <Input fullWidth value={email} onChange={(event) => setEmail(event.target.value)} type="email" autoFocus required variant="secondary" />
        </Field>
        <Field label="Password">
          <Input fullWidth value={password} onChange={(event) => setPassword(event.target.value)} type="password" required variant="secondary" />
        </Field>
        {mutation.error ? <ErrorBlock error={mutation.error} /> : null}
        <Button isDisabled={mutation.isPending} size="sm" type="submit" variant="primary">
          <Check size={16} />
          {mode === "login" ? "Sign in" : "Create account"}
        </Button>
        {oidcName ? (
          <Link className="auth-provider-link" href={api.oidcStartUrl({ returnTo: "/" })}>
            Continue with {oidcName}
          </Link>
        ) : null}
        {registrationMode === "invite_only" ? <p className="muted auth-note">New accounts require an app invitation.</p> : null}
      </form>
    </AuthFrame>
  );
}

function InvitationScreen({ authenticated, token }: { authenticated: boolean; token: string }) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const setup = useQuery({ queryKey: ["setup-status"], queryFn: () => api.setupStatus() });
  const invitation = useQuery({ queryKey: ["invitation", token], queryFn: () => api.invitation(token), retry: false });
  const [mode, setMode] = useState<"login" | "register">("login");
  const [password, setPassword] = useState("");
  const [displayName, setDisplayName] = useState("");
  const accept = useMutation({
    mutationFn: () => api.acceptInvitation(token),
    onSuccess: async (result) => {
      localStorage.setItem("revtern_app_id", result.app_id);
      await queryClient.invalidateQueries();
      navigate("/", { replace: true });
    },
  });
  const credentials = useMutation({
    mutationFn: async () => {
      const email = invitation.data?.invitation.email ?? "";
      if (mode === "register") {
        return api.register({ email, password, display_name: displayName || undefined, invite_token: token });
      }
      await api.login({ email, password });
      return api.acceptInvitation(token);
    },
    onSuccess: async (result) => {
      if ("app_id" in result) localStorage.setItem("revtern_app_id", result.app_id);
      if ("accepted_app_id" in result && result.accepted_app_id) localStorage.setItem("revtern_app_id", result.accepted_app_id);
      await queryClient.invalidateQueries();
      navigate("/", { replace: true });
    },
  });
  const preview = invitation.data?.invitation;
  const registrationMode = setup.data?.registration_mode ?? "invite_only";

  return (
    <AuthFrame>
      <div className="stack invitation-screen">
        <div>
          <span className="eyebrow">App invitation</span>
          <h1>{preview ? `Join ${preview.app_name}` : "App invitation"}</h1>
          {preview ? (
            <p className="muted">
              {preview.inviter_name ?? "An app manager"} invited {preview.email} with {titleize(preview.role)} access.
            </p>
          ) : null}
        </div>
        {invitation.isLoading ? <p className="muted">Checking invitation…</p> : null}
        {invitation.error ? <ErrorBlock error={invitation.error} /> : null}
        {preview && authenticated ? (
          <>
            {accept.error ? <ErrorBlock error={accept.error} /> : null}
            <Button isDisabled={accept.isPending} onPress={() => accept.mutate()} size="sm" variant="primary">
              <Check size={16} />
              Accept invitation
            </Button>
          </>
        ) : null}
        {preview && !authenticated ? (
          <form
            className="stack"
            onSubmit={(event) => {
              event.preventDefault();
              credentials.mutate();
            }}
          >
            {registrationMode !== "closed" ? (
              <Segmented value={mode} onChange={(value) => setMode(value as "login" | "register")} options={["login", "register"]} />
            ) : null}
            <Field label="Email">
              <Input fullWidth value={preview.email} readOnly variant="secondary" />
            </Field>
            {mode === "register" ? (
              <Field label="Display name">
                <Input fullWidth value={displayName} onChange={(event) => setDisplayName(event.target.value)} variant="secondary" />
              </Field>
            ) : null}
            <Field label="Password">
              <Input fullWidth value={password} onChange={(event) => setPassword(event.target.value)} type="password" minLength={8} required variant="secondary" />
            </Field>
            {credentials.error ? <ErrorBlock error={credentials.error} /> : null}
            <Button isDisabled={credentials.isPending} size="sm" type="submit" variant="primary">
              <Check size={16} />
              {mode === "register" ? "Create account and join" : "Sign in and join"}
            </Button>
            {setup.data?.oidc ? (
              <Link
                className="auth-provider-link"
                href={api.oidcStartUrl({ inviteToken: token, returnTo: `/invitations/${token}` })}
              >
                Continue with {setup.data.oidc.name}
              </Link>
            ) : null}
          </form>
        ) : null}
      </div>
    </AuthFrame>
  );
}

function invitationTokenFromPath(pathname: string) {
  const match = pathname.match(/^\/invitations\/([^/]+)\/?$/);
  return match ? decodeURIComponent(match[1]) : null;
}

interface AppScopeValue {
  apps: AppRecord[];
  selectedApp?: AppRecord;
  selectedAppId: string;
  setSelectedAppId: (appId: string) => void;
}

const AppScopeContext = createContext<AppScopeValue | null>(null);

function useAppScope() {
  const value = useContext(AppScopeContext);
  if (!value) throw new Error("App scope is unavailable");
  return value;
}

function AppShell({ me }: { me: { user: { email: string; role: string }; workspace: { name: string } } }) {
  const queryClient = useQueryClient();
  const location = useLocation();
  const navigate = useNavigate();
  const appsQuery = useQuery({ queryKey: ["apps"], queryFn: () => api.apps() });
  const apps = appsQuery.data?.apps ?? [];
  const [selectedAppId, setSelectedAppIdState] = useState(() => localStorage.getItem("revtern_app_id") ?? "");
  const selectedApp = apps.find((app) => app.id === selectedAppId);
  useEffect(() => {
    if (appsQuery.isSuccess && !apps.length && location.pathname !== "/settings") {
      navigate("/settings", { replace: true });
      return;
    }
    const acceptedAppId = new URLSearchParams(location.search).get("app_id");
    if (acceptedAppId && apps.some((app) => app.id === acceptedAppId)) {
      setSelectedAppIdState(acceptedAppId);
      localStorage.setItem("revtern_app_id", acceptedAppId);
      window.history.replaceState({}, "", location.pathname);
      return;
    }
    if (apps.length && !selectedApp) {
      setSelectedAppIdState(apps[0].id);
      localStorage.setItem("revtern_app_id", apps[0].id);
    }
  }, [apps, appsQuery.isSuccess, location.pathname, location.search, navigate, selectedApp]);
  const setSelectedAppId = (appId: string) => {
    setSelectedAppIdState(appId);
    localStorage.setItem("revtern_app_id", appId);
  };
  const logout = useMutation({
    mutationFn: () => api.logout(),
    onSuccess: async () => {
      await queryClient.invalidateQueries();
    },
  });

  return (
    <AppScopeContext.Provider value={{ apps, selectedApp, selectedAppId, setSelectedAppId }}>
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand-row sidebar-brand">
          <span className="brand-mark">R</span>
          <div>
            <strong>Revtern</strong>
            <span>{me.workspace.name}</span>
          </div>
        </div>
        {apps.length ? (
          <div className="sidebar-app-switcher">
            <span>Current app</span>
            <SelectControl
              ariaLabel="Current app"
              value={selectedAppId}
              onChange={setSelectedAppId}
              options={apps.map((app) => ({ value: app.id, label: app.name }))}
            />
            {selectedApp ? <small>{titleize(selectedApp.role)} access</small> : null}
          </div>
        ) : null}
        <nav className="nav-list">
          {navItems.filter((item) => !item.permission || selectedApp?.permissions.includes(item.permission)).map((item) => {
            const Icon = item.icon;
            return (
              <RouterNavLink
                key={item.to}
                to={item.to}
                end={item.to === "/"}
                className={({ isActive }) => (isActive ? "nav-link active" : "nav-link")}
              >
                <Icon size={17} />
                {item.label}
              </RouterNavLink>
            );
          })}
        </nav>
        <div className="sidebar-footer">
          <span>{me.user.email}</span>
          <Button className="sidebar-action" isDisabled={logout.isPending} onPress={() => logout.mutate()} size="sm" variant="ghost">
            <LogOut size={16} />
            Log out
          </Button>
        </div>
      </aside>
      <main className="main-area">
        <Routes>
          <Route path="/" element={<OverviewPage />} />
          <Route path="/revenue" element={<RevenuePage />} />
          <Route path="/transactions" element={<TransactionsPage />} />
          <Route path="/subscriptions" element={<SubscriptionsPage />} />
          <Route path="/products" element={<ProductsPage />} />
          <Route path="/events" element={<EventsPage />} />
          <Route path="/sources" element={<SourcesPage />} />
          <Route path="/reconciliation" element={<ReconciliationPage />} />
          <Route path="/jobs" element={<JobsPage />} />
          <Route path="/settings" element={<SettingsPage />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </main>
    </div>
    </AppScopeContext.Provider>
  );
}

function OverviewPage() {
  const queryClient = useQueryClient();
  const [filters, setFilters] = useDashboardFilters();
  const overview = useQuery({ queryKey: ["overview", filters], queryFn: () => api.overview(filters) });
  const series = useQuery({ queryKey: ["revenue-series", filters], queryFn: () => api.revenueTimeseries(filters) });
  const transactions = useQuery({ queryKey: ["transactions", "recent", filters], queryFn: () => api.transactions({ ...filters }), placeholderData: (previous) => previous });
  const sources = useQuery({ queryKey: ["data-sources", filters.app_id], queryFn: () => api.dataSources({ app_id: filters.app_id }) });
  const sourceProducts = useQuery({ queryKey: ["source-products", "overview", filters.app_id], queryFn: () => api.sourceProducts({ app_id: filters.app_id }) });
  const seed = useMutation({
    mutationFn: () => api.seedDemo(),
    onSuccess: async () => {
      await queryClient.invalidateQueries();
    },
  });
  const revenueSeries = series.data?.series ?? [];
  const warningCount = overview.data?.warnings.length ?? 0;
  const sourceRows = sources.data?.data_sources ?? [];
  const sourceIssueCount = sourceRows.filter((source) => ["error", "failed", "dead"].includes(source.status.toLowerCase())).length;
  const unmappedCount = (sourceProducts.data?.source_products ?? []).filter((product) => product.mapping_state === "unmapped").length;
  const trustState = overview.data?.metrics.gross_revenue_minor.trust_state ?? "estimated";
  const metrics = overview.data?.metrics;
  const supportingMetrics = [
    {
      label: "Net revenue",
      value: formatMoney(metrics?.net_revenue_minor.value, overview.data?.currency),
      state: metrics?.net_revenue_minor.trust_state,
      delta: "Gross − refunds",
    },
    {
      label: "Active subscriptions",
      value: formatNumber(metrics?.active_subscriptions.value),
      state: metrics?.active_subscriptions.trust_state,
      delta: `${formatNumber(metrics?.new_subscriptions.value)} new`,
    },
    {
      label: "Renewals",
      value: formatNumber(metrics?.renewals.value),
      state: metrics?.renewals.trust_state,
      delta: "Period total",
    },
    {
      label: "Refunds",
      value: formatMoney(metrics?.refund_amount_minor.value, overview.data?.currency),
      state: metrics?.refund_amount_minor.trust_state,
      delta: `${formatPercent(metrics?.refund_rate.value)} of gross`,
    },
    {
      label: "Churned",
      value: formatNumber(metrics?.churned_subscriptions.value),
      state: metrics?.churned_subscriptions.trust_state,
      delta: "Period total",
    },
  ];

  return (
    <Page title="Overview" actions={<FilterBar filters={filters} onChange={setFilters} />}>
      <div className="overview-content">
        {overview.error ? <ErrorBlock error={overview.error} /> : null}
        <div className="overview-grid">
          <Card className="overview-panel revenue-stage" variant="secondary">
            <Card.Content className="revenue-stage__content">
              <div className="stage-head">
                <div>
                  <span className="stage-kicker"><TrendingUp size={15} /> Gross revenue</span>
                  <h2>{formatMoney(metrics?.gross_revenue_minor.value, overview.data?.currency)}</h2>
                </div>
                <div className="stage-proof">
                  <StatusLabel value={trustState} />
                  <span>{formatDate(overview.data?.period.from)} – {formatDate(overview.data?.period.to)}</span>
                </div>
              </div>
              <ChartFrame>
                <ResponsiveContainer width="100%" height={288}>
                  <LineChart data={revenueSeries} margin={{ top: 14, right: 18, bottom: 0, left: 0 }}>
                    <CartesianGrid stroke="var(--separator)" strokeDasharray="2 6" vertical={false} />
                    <XAxis dataKey="date" tickFormatter={formatDate} tickLine={false} axisLine={{ stroke: "var(--separator)" }} stroke="var(--muted)" />
                    <YAxis tickFormatter={(value) => formatCompactMoney(Number(value), overview.data?.currency)} tickLine={false} axisLine={false} width={68} stroke="var(--muted)" />
                    <Tooltip
                      formatter={(value) => formatMoney(Number(value), overview.data?.currency)}
                      labelFormatter={(value) => formatDate(String(value))}
                      contentStyle={{ background: "var(--overlay)", border: "1px solid var(--separator)", borderRadius: 12, color: "var(--overlay-foreground)" }}
                      labelStyle={{ color: "var(--accent)" }}
                    />
                    <Line dataKey="gross_revenue_minor" name="Gross" stroke="var(--accent)" strokeWidth={2.4} dot={false} activeDot={{ r: 4, fill: "var(--accent)" }} isAnimationActive={false} />
                    <Line dataKey="net_revenue_minor" name="Net" stroke="var(--success)" strokeWidth={1.7} dot={false} isAnimationActive={false} />
                  </LineChart>
                </ResponsiveContainer>
              </ChartFrame>
              <div className="stage-ledger-grid">
                {supportingMetrics.map((metric) => (
                  <OverviewMetric key={metric.label} {...metric} />
                ))}
              </div>
            </Card.Content>
          </Card>
          <aside className="overview-side" aria-label="Revenue evidence">
            <Card className="overview-panel source-ledger" variant="secondary">
              <Card.Header className="overview-panel-head">
                <Card.Title>Source health</Card.Title>
                <AppLink to="/sources">View all <ArrowUpRight size={14} /></AppLink>
              </Card.Header>
              <Card.Content className="source-ledger-table">
                {sourceRows.slice(0, 6).map((source) => (
                  <div className="source-ledger-row" key={source.id}>
                    <div>
                      <strong>{source.name}</strong>
                      <span>{titleize(source.source_type)} · {source.app_name ?? "No app"}</span>
                    </div>
                    <StatusLabel value={source.status} />
                    <span className="last-sync">{formatDateTime(source.last_sync_at ?? source.last_event_at ?? source.updated_at)}</span>
                  </div>
                ))}
                {!sourceRows.length ? (
                  <EmptyState
                    icon={<RadioTower size={18} />}
                    title="No source connected"
                    action={<AppLink to="/sources"><Plus size={16} /> Add source</AppLink>}
                  />
                ) : null}
              </Card.Content>
            </Card>
            <Card className="overview-panel reconciliation-card" variant="secondary">
              <Card.Header className="overview-panel-head">
                <Card.Title>Reconciliation</Card.Title>
                <ShieldCheck size={17} />
              </Card.Header>
              <Card.Content className="reconciliation-card__content">
                <dl className="reconciliation-list">
                  <div>
                    <dt>Trust state</dt>
                    <dd><StatusLabel value={warningCount || sourceIssueCount || unmappedCount ? "estimated" : trustState} /></dd>
                  </div>
                  <div>
                    <dt>Metric notes</dt>
                    <dd>{formatNumber(warningCount)}</dd>
                  </div>
                  <div>
                    <dt>Unmapped products</dt>
                    <dd>{formatNumber(unmappedCount)}</dd>
                  </div>
                  <div>
                    <dt>Source issues</dt>
                    <dd>{formatNumber(sourceIssueCount)}</dd>
                  </div>
                </dl>
                {overview.data?.warnings.length ? (
                  <ul className="issue-list compact-issues">
                    {overview.data.warnings.slice(0, 3).map((warning) => (
                      <li key={warning}>
                        <AlertTriangle size={15} />
                        <span>{warning}</span>
                      </li>
                    ))}
                  </ul>
                ) : (
                  <p className="quiet-copy">No blocking metric issues detected for this range.</p>
                )}
              </Card.Content>
            </Card>
          </aside>
        </div>
        <Card className="overview-panel overview-ledger" variant="secondary">
          <Card.Header className="overview-panel-head">
            <Card.Title>Recent transactions</Card.Title>
            {!transactions.data?.transactions.length ? (
              <Button isDisabled={seed.isPending} onPress={() => seed.mutate()} size="sm" variant="secondary">
                <Plus size={16} />
                Seed demo
              </Button>
            ) : (
              <AppLink to="/transactions">View all transactions <ArrowUpRight size={14} /></AppLink>
            )}
          </Card.Header>
          <Card.Content className="overview-ledger__content">
            <TransactionTable transactions={(transactions.data?.transactions ?? []).slice(0, 8)} compact />
          </Card.Content>
        </Card>
      </div>
    </Page>
  );
}

function OverviewMetric({ delta, label, state, value }: { delta: string; label: string; state?: string; value: string }) {
  return (
    <div className="overview-metric">
      <span>{label}</span>
      <strong>{value}</strong>
      <div>
        <StatusLabel value={state ?? "estimated"} />
        <small>{delta}</small>
      </div>
    </div>
  );
}

function RevenuePage() {
  const [filters, setFilters] = useDashboardFilters();
  const [by, setBy] = useState("product");
  const series = useQuery({ queryKey: ["revenue-series", filters], queryFn: () => api.revenueTimeseries(filters) });
  const breakdown = useQuery({ queryKey: ["breakdown", filters, by], queryFn: () => api.breakdown({ ...filters, by }) });

  return (
    <Page title="Revenue" actions={<FilterBar filters={filters} onChange={setFilters} />}>
      <Panel title="Revenue by day">
        <ChartFrame>
          <ResponsiveContainer width="100%" height={300}>
            <BarChart data={series.data?.series ?? []}>
              <CartesianGrid strokeDasharray="3 3" vertical={false} />
              <XAxis dataKey="date" tickFormatter={formatDate} />
              <YAxis tickFormatter={(value) => formatCompactMoney(Number(value), filters.currency)} />
              <Tooltip formatter={(value) => formatMoney(Number(value), filters.currency)} labelFormatter={(value) => formatDate(String(value))} />
              <Legend />
              <Bar dataKey="gross_revenue_minor" name="Gross" fill="var(--accent)" isAnimationActive={false} />
              <Bar dataKey="refund_amount_minor" name="Refunds" fill="var(--danger)" isAnimationActive={false} />
            </BarChart>
          </ResponsiveContainer>
        </ChartFrame>
      </Panel>
      <Panel
        title="Breakdown"
        actions={
          <Segmented value={by} onChange={setBy} options={["product", "app", "platform", "country", "source"]} />
        }
      >
        <DataTable ariaLabel="Revenue breakdown">
          <Table.Header>
            <Table.Column isRowHeader>{titleize(by)}</Table.Column>
            <Table.Column>Gross</Table.Column>
            <Table.Column>Refunds</Table.Column>
            <Table.Column>Transactions</Table.Column>
          </Table.Header>
          <Table.Body>
            {(breakdown.data?.items ?? []).map((item) => (
              <Table.Row id={item.label} key={item.label}>
                <Table.Cell>{item.label}</Table.Cell>
                <Table.Cell>{formatMoney(item.gross_revenue_minor, filters.currency)}</Table.Cell>
                <Table.Cell>{formatMoney(item.refund_amount_minor, filters.currency)}</Table.Cell>
                <Table.Cell>{formatNumber(item.transaction_count)}</Table.Cell>
              </Table.Row>
            ))}
          </Table.Body>
        </DataTable>
      </Panel>
    </Page>
  );
}

function TransactionsPage() {
  const [filters, setFilters] = useDashboardFilters();
  const [status, setStatus] = useState("all");
  const [environment, setEnvironment] = useState("all");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const transactions = useQuery({
    queryKey: ["transactions", filters, status, environment],
    queryFn: () => api.transactions({ ...filters, status, environment }),
  });
  const detail = useQuery({
    queryKey: ["transaction", selectedId],
    queryFn: () => api.transaction(selectedId!),
    enabled: Boolean(selectedId),
  });
  return (
    <Page
      title="Transactions"
      actions={
        <Toolbar className="toolbar" aria-label="Transaction filters">
          <FilterBar filters={filters} onChange={setFilters} compact />
          <SelectControl
            ariaLabel="Transaction status"
            value={status}
            onChange={setStatus}
            options={[
              { value: "all", label: "All statuses" },
              { value: "paid", label: "Paid" },
              { value: "renewed", label: "Renewed" },
              { value: "refunded", label: "Refunded" },
              { value: "revoked", label: "Revoked" },
            ]}
          />
          <SelectControl
            ariaLabel="Transaction environment"
            value={environment}
            onChange={setEnvironment}
            options={[
              { value: "all", label: "All environments" },
              { value: "production", label: "Production" },
              { value: "sandbox", label: "Apple sandbox" },
              { value: "test", label: "Test purchases" },
              { value: "unknown", label: "Unverified" },
            ]}
          />
        </Toolbar>
      }
    >
      <Panel title="Ledger">
        {transactions.error ? <ErrorBlock error={transactions.error} /> : null}
        <TransactionTable transactions={transactions.data?.transactions ?? []} onSelect={setSelectedId} />
      </Panel>
      {selectedId ? (
        <EvidenceDrawer
          title={detail.data?.transaction.transaction_key ?? "Transaction evidence"}
          subtitle={detail.data ? `${detail.data.transaction.logical_product_name ?? detail.data.transaction.source_product_name ?? "Unmapped"} · ${formatMoney(detail.data.transaction.amount_minor, detail.data.transaction.currency)}` : undefined}
          events={detail.data?.events ?? []}
          error={detail.error}
          loading={detail.isLoading}
          onClose={() => setSelectedId(null)}
        />
      ) : null}
    </Page>
  );
}

function SubscriptionsPage() {
  const [filters, setFilters] = useDashboardFilters();
  const [status, setStatus] = useState("all");
  const [environment, setEnvironment] = useState("production");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const subscriptions = useQuery({ queryKey: ["subscriptions", filters, status, environment], queryFn: () => api.subscriptions({ ...filters, status, environment }) });
  const subSeries = useQuery({ queryKey: ["subscription-series", filters], queryFn: () => api.subscriptionTimeseries(filters) });
  const detail = useQuery({ queryKey: ["subscription", selectedId], queryFn: () => api.subscription(selectedId!), enabled: Boolean(selectedId) });
  return (
    <Page
      title="Subscriptions"
      actions={
        <Toolbar className="toolbar" aria-label="Subscription filters">
          <FilterBar filters={filters} onChange={setFilters} compact />
          <SelectControl
            ariaLabel="Subscription status"
            value={status}
            onChange={setStatus}
            options={[
              { value: "all", label: "All statuses" },
              { value: "trialing", label: "Trialing" },
              { value: "active", label: "Active" },
              { value: "cancelled_active", label: "Cancelled active" },
              { value: "grace_period", label: "Grace period" },
              { value: "billing_retry", label: "Billing retry" },
              { value: "expired", label: "Expired" },
            ]}
          />
          <SelectControl
            ariaLabel="Subscription environment"
            value={environment}
            onChange={setEnvironment}
            options={[
              { value: "all", label: "All environments" },
              { value: "production", label: "Production" },
              { value: "sandbox", label: "Apple sandbox" },
              { value: "test", label: "Test purchases" },
              { value: "unknown", label: "Unverified" },
            ]}
          />
        </Toolbar>
      }
    >
      <Panel title="Subscription movement">
        <ChartFrame>
          <ResponsiveContainer width="100%" height={240}>
            <LineChart data={subSeries.data?.series ?? []}>
              <CartesianGrid strokeDasharray="3 3" vertical={false} />
              <XAxis dataKey="date" tickFormatter={formatDate} />
              <YAxis />
              <Tooltip labelFormatter={(value) => formatDate(String(value))} />
              <Legend />
              <Line dataKey="new_subscription_count" name="New" stroke="var(--success)" strokeWidth={2} dot={false} isAnimationActive={false} />
              <Line dataKey="renewal_count" name="Renewals" stroke="var(--accent)" strokeWidth={2} dot={false} isAnimationActive={false} />
              <Line dataKey="cancel_count" name="Cancels" stroke="var(--warning)" strokeWidth={2} dot={false} isAnimationActive={false} />
            </LineChart>
          </ResponsiveContainer>
        </ChartFrame>
      </Panel>
      <Panel title="Current subscriptions">
        <DataTable ariaLabel="Current subscriptions">
          <Table.Header>
            <Table.Column isRowHeader>Subscription</Table.Column>
            <Table.Column>Product</Table.Column>
            <Table.Column>Platform</Table.Column>
            <Table.Column>Environment</Table.Column>
            <Table.Column>Status</Table.Column>
            <Table.Column>Started</Table.Column>
            <Table.Column>Renewal</Table.Column>
          </Table.Header>
          <Table.Body>
            {(subscriptions.data?.subscriptions ?? []).map((subscription) => (
              <Table.Row className="click-row" id={subscription.id} key={subscription.id} onAction={() => setSelectedId(subscription.id)}>
                <Table.Cell className="mono-cell">{subscription.subscription_key}</Table.Cell>
                <Table.Cell>{subscription.logical_product_name ?? subscription.source_product_name ?? "Unmapped"}</Table.Cell>
                <Table.Cell>{titleize(subscription.platform)}</Table.Cell>
                <Table.Cell><StatusLabel value={subscription.environment} /></Table.Cell>
                <Table.Cell><StatusLabel value={subscription.status} /></Table.Cell>
                <Table.Cell>{formatDateTime(subscription.started_at)}</Table.Cell>
                <Table.Cell>{subscription.will_renew ? "Will renew" : "Won't renew"}</Table.Cell>
              </Table.Row>
            ))}
          </Table.Body>
        </DataTable>
      </Panel>
      {selectedId ? (
        <EvidenceDrawer
          title={detail.data?.subscription.subscription_key ?? "Subscription evidence"}
          subtitle={detail.data ? `${detail.data.subscription.logical_product_name ?? detail.data.subscription.source_product_name ?? "Unmapped"} · ${titleize(detail.data.subscription.status)}` : undefined}
          events={detail.data?.timeline ?? []}
          error={detail.error}
          loading={detail.isLoading}
          onClose={() => setSelectedId(null)}
          subscription={detail.data?.subscription}
        />
      ) : null}
    </Page>
  );
}

function ProductsPage() {
  const queryClient = useQueryClient();
  const { selectedApp, selectedAppId } = useAppScope();
  const sourceProducts = useQuery({ queryKey: ["source-products", selectedAppId], queryFn: () => api.sourceProducts({ app_id: selectedAppId }), enabled: Boolean(selectedAppId) });
  const logicalProducts = useQuery({ queryKey: ["logical-products", selectedAppId], queryFn: () => api.logicalProducts({ app_id: selectedAppId }), enabled: Boolean(selectedAppId) });
  const [drafts, setDrafts] = useState<CatalogDraftGroup[]>([]);
  const [ignored, setIgnored] = useState<Set<string>>(new Set());
  const unmapped = useMemo(
    () => (sourceProducts.data?.source_products ?? []).filter((product) => product.mapping_state === "unmapped"),
    [sourceProducts.data],
  );

  useEffect(() => {
    setDrafts(buildCatalogDraft(unmapped));
    setIgnored(new Set());
  }, [unmapped.map((product) => product.id).join(",")]);

  const confirm = useMutation({
    mutationFn: () => {
      const payload: CatalogConfirmation = {
        app_id: selectedAppId,
        logical_products: drafts
          .filter((draft) => draft.source_product_ids.some((id) => !ignored.has(id)))
          .map((draft) => ({
            client_id: draft.client_id,
            display_name: draft.display_name,
            product_kind: draft.product_kind,
            billing_period: draft.billing_period,
            reporting_category: draft.reporting_category,
          })),
        mappings: drafts.flatMap((draft) =>
          draft.source_product_ids
            .filter((id) => !ignored.has(id))
            .map((id) => ({
              source_product_id: id,
              logical_product_client_id: draft.client_id,
              mapping_method: "user_confirmed_catalog_draft",
            })),
        ),
        ignored_source_product_ids: [...ignored],
      };
      return api.confirmCatalog(payload);
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["source-products"] });
      await queryClient.invalidateQueries({ queryKey: ["logical-products"] });
      await queryClient.invalidateQueries({ queryKey: ["overview"] });
    },
  });

  return (
    <Page
      title="Products"
      actions={
        <Toolbar className="toolbar" aria-label="Product catalog actions">
          <Button onPress={() => setDrafts(buildCatalogDraft(unmapped))} size="sm" variant="secondary">
            <RefreshCw size={16} />
            Regenerate draft
          </Button>
          <Button
            isDisabled={!selectedAppId || !selectedApp?.permissions.includes("catalog.write") || confirm.isPending || (!drafts.length && !ignored.size)}
            onPress={() => confirm.mutate()}
            size="sm"
            variant="primary"
          >
            <Check size={16} />
            Confirm catalog
          </Button>
        </Toolbar>
      }
    >
      {confirm.error ? <ErrorBlock error={confirm.error} /> : null}
      <Panel title="Catalog draft">
        {!unmapped.length ? (
          <EmptyState icon={<Boxes size={18} />} title="No unmapped source products" />
        ) : (
          <div className="draft-list">
            {drafts.map((draft, index) => (
              <section className="draft-item" key={draft.client_id}>
                <div className="draft-item__content">
                  <div className="draft-form-grid">
                    <Field label="Product name">
                      <Input fullWidth value={draft.display_name} onChange={(event) => updateDraft(setDrafts, index, { display_name: event.target.value })} variant="secondary" />
                    </Field>
                    <Field label="Kind">
                      <SelectControl
                        ariaLabel="Product kind"
                        value={draft.product_kind}
                        onChange={(value) => updateDraft(setDrafts, index, { product_kind: value })}
                        options={[
                          { value: "subscription", label: "Subscription" },
                          { value: "consumable", label: "Consumable" },
                          { value: "non_consumable", label: "Non-consumable" },
                          { value: "lifetime", label: "Lifetime" },
                          { value: "unknown", label: "Unknown" },
                        ]}
                      />
                    </Field>
                    <Field label="Period">
                      <SelectControl
                        ariaLabel="Billing period"
                        value={draft.billing_period}
                        onChange={(value) => updateDraft(setDrafts, index, { billing_period: value })}
                        options={[
                          { value: "weekly", label: "Weekly" },
                          { value: "monthly", label: "Monthly" },
                          { value: "annual", label: "Annual" },
                          { value: "lifetime", label: "Lifetime" },
                          { value: "none", label: "None" },
                          { value: "unknown", label: "Unknown" },
                        ]}
                      />
                    </Field>
                    <Field label="Category">
                      <Input fullWidth value={draft.reporting_category} onChange={(event) => updateDraft(setDrafts, index, { reporting_category: event.target.value })} variant="secondary" />
                    </Field>
                  </div>
                  <span className="muted">{draft.reason}</span>
                  <div className="source-product-list">
                    {draft.source_product_ids.map((id) => {
                      const product = unmapped.find((item) => item.id === id);
                      if (!product) return null;
                      return (
                        <Checkbox
                          className="source-product-row"
                          isSelected={!ignored.has(id)}
                          key={id}
                          onChange={(checked) => {
                            const next = new Set(ignored);
                            if (checked) next.delete(id);
                            else next.add(id);
                            setIgnored(next);
                          }}
                          variant="secondary"
                        >
                          <Checkbox.Content className="source-product-row__content">
                            <Checkbox.Control><Checkbox.Indicator /></Checkbox.Control>
                            <span className="source-product-name">{product.external_product_id ?? product.display_name ?? id}</span>
                            <span className="source-product-meta">{titleize(product.source_type)} · {titleize(product.product_kind)} · {titleize(product.billing_period)}</span>
                          </Checkbox.Content>
                        </Checkbox>
                      );
                    })}
                  </div>
                </div>
              </section>
            ))}
          </div>
        )}
      </Panel>
      <Panel title="Confirmed products">
        <ProductTable products={logicalProducts.data?.logical_products ?? []} />
      </Panel>
    </Page>
  );
}

function EventsPage() {
  const { selectedAppId } = useAppScope();
  const [tab, setTab] = useState<"raw" | "normalized">("raw");
  const [q, setQ] = useState("");
  const raw = useQuery({ queryKey: ["raw-events", selectedAppId, q], queryFn: () => api.rawEvents({ app_id: selectedAppId, q }), enabled: tab === "raw" && Boolean(selectedAppId) });
  const normalized = useQuery({ queryKey: ["normalized-events", selectedAppId], queryFn: () => api.normalizedEvents({ app_id: selectedAppId }), enabled: tab === "normalized" && Boolean(selectedAppId) });
  const [selected, setSelected] = useState<RawEventRecord | null>(null);

  return (
    <Page
      title="Events"
      actions={
        <Toolbar className="toolbar" aria-label="Event filters">
          <Segmented value={tab} onChange={(value) => setTab(value as "raw" | "normalized")} options={["raw", "normalized"]} />
          <div className="search-control">
            <Search size={16} />
            <Input
              aria-label="Search raw payloads"
              className="search-input"
              disabled={tab !== "raw"}
              onChange={(event) => setQ(event.target.value)}
              placeholder="Search raw payloads"
              value={q}
              variant="secondary"
            />
          </div>
        </Toolbar>
      }
    >
      {tab === "raw" ? (
        <Panel title="Raw events">
          <DataTable ariaLabel="Raw events">
            <Table.Header>
              <Table.Column isRowHeader>Received</Table.Column>
              <Table.Column>Source</Table.Column>
              <Table.Column>Event</Table.Column>
              <Table.Column>Environment</Table.Column>
              <Table.Column>Product</Table.Column>
              <Table.Column>Status</Table.Column>
              <Table.Column>Signature</Table.Column>
            </Table.Header>
            <Table.Body>
              {(raw.data?.raw_events ?? []).map((event) => (
                <Table.Row className="click-row" id={event.id} key={event.id} onAction={() => setSelected(event)}>
                  <Table.Cell>{formatDateTime(event.received_at)}</Table.Cell>
                  <Table.Cell>{titleize(event.source_type)}</Table.Cell>
                  <Table.Cell>{event.source_event_type ?? event.source_event_id}</Table.Cell>
                  <Table.Cell><StatusLabel value={event.environment} /></Table.Cell>
                  <Table.Cell>{event.source_product_name ?? event.source_product_id ?? "—"}</Table.Cell>
                  <Table.Cell><StatusLabel value={event.processing_status} /></Table.Cell>
                  <Table.Cell>{event.signature_verified ? "Verified" : "Stored"}</Table.Cell>
                </Table.Row>
              ))}
            </Table.Body>
          </DataTable>
          {selected ? <JsonDrawer title={selected.source_event_id} value={selected.payload} onClose={() => setSelected(null)} /> : null}
        </Panel>
      ) : (
        <Panel title="Normalized events">
          <DataTable ariaLabel="Normalized events">
            <Table.Header>
              <Table.Column isRowHeader>Time</Table.Column>
              <Table.Column>Type</Table.Column>
              <Table.Column>Environment</Table.Column>
              <Table.Column>Product</Table.Column>
              <Table.Column>Transaction</Table.Column>
              <Table.Column>Amount</Table.Column>
              <Table.Column>Confidence</Table.Column>
            </Table.Header>
            <Table.Body>
              {(normalized.data?.normalized_events ?? []).map((event) => (
                <Table.Row id={event.id} key={event.id}>
                  <Table.Cell>{formatDateTime(event.occurred_at)}</Table.Cell>
                  <Table.Cell>{titleize(event.event_type)}</Table.Cell>
                  <Table.Cell><StatusLabel value={event.environment} /></Table.Cell>
                  <Table.Cell>{event.logical_product_name ?? event.source_product_name ?? "Unmapped"}</Table.Cell>
                  <Table.Cell className="mono-cell">{event.transaction_key ?? "—"}</Table.Cell>
                  <Table.Cell>{formatMoney(event.amount_minor, event.currency ?? "USD")}</Table.Cell>
                  <Table.Cell>{Math.round(event.confidence * 100)}%</Table.Cell>
                </Table.Row>
              ))}
            </Table.Body>
          </DataTable>
        </Panel>
      )}
    </Page>
  );
}

function SourcesPage() {
  const queryClient = useQueryClient();
  const { apps, selectedApp, selectedAppId } = useAppScope();
  const writableApps = apps.filter((app) => app.permissions.includes("source.write"));
  const sources = useQuery({ queryKey: ["data-sources", selectedAppId], queryFn: () => api.dataSources({ app_id: selectedAppId }), enabled: Boolean(selectedAppId) });
  const create = useMutation({
    mutationFn: (input: { source_type: string; name: string; app_id?: string | null; credentials?: Record<string, unknown> }) => api.createDataSource(input),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["data-sources"] });
    },
  });
  const test = useMutation({
    mutationFn: (id: string) => api.testDataSource(id),
    onSuccess: async () => queryClient.invalidateQueries({ queryKey: ["data-sources"] }),
  });
  const appStoreTest = useMutation({
    mutationFn: (input: { id: string; environment: AppStoreTestEnvironment }) =>
      api.sendAppStoreTestNotification(input.id, input.environment),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["data-sources"] });
      window.setTimeout(() => {
        void queryClient.invalidateQueries({ queryKey: ["data-sources"] });
      }, 2500);
    },
  });
  const catchUp = useMutation({
    mutationFn: (id: string) => api.catchUpDataSource(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["data-sources"] });
      await queryClient.invalidateQueries({ queryKey: ["events"] });
      await queryClient.invalidateQueries({ queryKey: ["overview"] });
    },
  });
  const updateCredentials = useMutation({
    mutationFn: (input: { id: string; credentials: Record<string, unknown> }) =>
      api.updateDataSourceCredentials(input.id, { credentials: input.credentials }),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["data-sources"] });
    },
  });

  return (
    <Page title="Sources">
      <div className="two-column align-start">
        <Panel title="Add source">
          {writableApps.length ? (
            <SourceForm
              allowCredentials={Boolean(selectedApp?.permissions.includes("source.credentials.write"))}
              apps={writableApps}
              initialAppId={selectedAppId}
              onSubmit={(input) => create.mutate(input)}
              pending={create.isPending}
              error={create.error}
            />
          ) : (
            <p className="muted">Your role can view source health but cannot add or change sources.</p>
          )}
        </Panel>
        <Panel title="Connected sources">
          <div className="source-card-list">
            {(sources.data?.data_sources ?? []).map((source) => (
              <SourceCard
                key={source.id}
                source={source}
                onTest={selectedApp?.permissions.includes("source.write") && source.source_type !== "app_store" ? () => test.mutate(source.id) : undefined}
                onSendAppStoreTest={selectedApp?.permissions.includes("source.write") && source.source_type === "app_store"
                  ? (environment) => appStoreTest.mutate({ id: source.id, environment })
                  : undefined}
                appStoreTestPending={appStoreTest.isPending && appStoreTest.variables?.id === source.id}
                appStoreTestResult={appStoreTest.variables?.id === source.id ? appStoreTest.data?.test_notification : undefined}
                appStoreTestError={appStoreTest.variables?.id === source.id ? appStoreTest.error : null}
                onConfigureCatchUp={selectedApp?.permissions.includes("source.credentials.write") && supportsCatchUp(source.source_type)
                  ? (credentials) => updateCredentials.mutate({ id: source.id, credentials })
                  : undefined}
                onCatchUp={selectedApp?.permissions.includes("source.write") && supportsCatchUp(source.source_type) && source.catch_up_configured ? () => catchUp.mutate(source.id) : undefined}
                configurePending={updateCredentials.isPending && updateCredentials.variables?.id === source.id}
                configureError={updateCredentials.variables?.id === source.id ? updateCredentials.error : null}
              />
            ))}
            {!sources.data?.data_sources.length ? <EmptyState icon={<RadioTower size={18} />} title="No sources yet" /> : null}
          </div>
        </Panel>
      </div>
    </Page>
  );
}

function ReconciliationPage() {
  const { selectedAppId } = useAppScope();
  const overview = useQuery({ queryKey: ["overview", "reconciliation", selectedAppId], queryFn: () => api.overview({ app_id: selectedAppId }), enabled: Boolean(selectedAppId) });
  const sourceProducts = useQuery({ queryKey: ["source-products", selectedAppId], queryFn: () => api.sourceProducts({ app_id: selectedAppId }), enabled: Boolean(selectedAppId) });
  const jobs = useQuery({ queryKey: ["jobs", selectedAppId], queryFn: () => api.jobs({ app_id: selectedAppId }), enabled: Boolean(selectedAppId) });
  const rawFailed = useQuery({ queryKey: ["raw-events", "failed", selectedAppId], queryFn: () => api.rawEvents({ app_id: selectedAppId, processing_status: "failed" }), enabled: Boolean(selectedAppId) });
  const issues = [
    ...(overview.data?.warnings ?? []).map((message) => ({ message, severity: "warning" })),
    ...((sourceProducts.data?.source_products ?? []).filter((product) => product.mapping_state === "unmapped").map((product) => ({
      message: `${product.external_product_id ?? product.id} is not mapped to a logical product.`,
      severity: "warning",
    }))),
    ...((jobs.data?.jobs ?? []).filter((job) => ["failed", "dead"].includes(job.status)).map((job) => ({
      message: `${job.job_type} ${job.id} is ${job.status}.`,
      severity: "error",
    }))),
    ...((rawFailed.data?.raw_events ?? []).map((event) => ({
      message: `${event.source_event_id} failed processing: ${event.processing_error ?? "unknown error"}`,
      severity: "error",
    }))),
  ];

  return (
    <Page title="Reconciliation">
      <Panel title="Open issues">
        {issues.length ? (
          <ul className="issue-list">
            {issues.map((issue, index) => (
              <li key={`${issue.message}-${index}`}>
                <AlertTriangle size={16} />
                <span>{issue.message}</span>
                <StatusLabel value={issue.severity} />
              </li>
            ))}
          </ul>
        ) : (
          <EmptyState icon={<Check size={18} />} title="No reconciliation issues detected" />
        )}
      </Panel>
    </Page>
  );
}

function JobsPage() {
  const queryClient = useQueryClient();
  const { selectedAppId } = useAppScope();
  const jobs = useQuery({ queryKey: ["jobs", selectedAppId], queryFn: () => api.jobs({ app_id: selectedAppId }), enabled: Boolean(selectedAppId), refetchInterval: 10_000 });
  const retry = useMutation({
    mutationFn: (id: string) => api.retryJob(id),
    onSuccess: async () => queryClient.invalidateQueries({ queryKey: ["jobs"] }),
  });

  return (
    <Page title="Jobs">
      <Panel title="Queue">
        <DataTable ariaLabel="Jobs queue">
          <Table.Header>
            <Table.Column isRowHeader>Created</Table.Column>
            <Table.Column>Type</Table.Column>
            <Table.Column>Status</Table.Column>
            <Table.Column>Attempts</Table.Column>
            <Table.Column>Error</Table.Column>
            <Table.Column>Action</Table.Column>
          </Table.Header>
          <Table.Body>
            {(jobs.data?.jobs ?? []).map((job) => (
              <Table.Row id={job.id} key={job.id}>
                <Table.Cell>{formatDateTime(job.created_at)}</Table.Cell>
                <Table.Cell>{job.job_type}</Table.Cell>
                <Table.Cell><StatusLabel value={job.status} /></Table.Cell>
                <Table.Cell>{job.attempts}/{job.max_attempts}</Table.Cell>
                <Table.Cell>{job.last_error ?? "—"}</Table.Cell>
                <Table.Cell>
                  {["failed", "dead"].includes(job.status) ? (
                    <Button aria-label="Retry job" isIconOnly onPress={() => retry.mutate(job.id)} size="sm" variant="secondary">
                      <RefreshCw size={16} />
                    </Button>
                  ) : null}
                </Table.Cell>
              </Table.Row>
            ))}
          </Table.Body>
        </DataTable>
      </Panel>
    </Page>
  );
}

function SettingsPage() {
  const queryClient = useQueryClient();
  const { apps, selectedApp, selectedAppId, setSelectedAppId } = useAppScope();
  const setup = useQuery({ queryKey: ["setup-status"], queryFn: () => api.setupStatus() });
  const identities = useQuery({ queryKey: ["auth-identities"], queryFn: () => api.authIdentities() });
  const createApp = useMutation({
    mutationFn: (input: { name: string; apple_bundle_id?: string; google_package_name?: string; default_currency?: string }) => api.createApp(input),
    onSuccess: async (result) => {
      setSelectedAppId(result.app.id);
      await queryClient.invalidateQueries({ queryKey: ["apps"] });
    },
  });
  const updateApp = useMutation({
    mutationFn: (input: { name: string; apple_bundle_id?: string; google_package_name?: string; default_currency?: string }) => api.updateApp(selectedAppId, input),
    onSuccess: async () => queryClient.invalidateQueries({ queryKey: ["apps"] }),
  });
  return (
    <Page title="Settings">
      <div className="settings-grid">
        <Panel title="Your apps">
          <AppForm onSubmit={(input) => createApp.mutate(input)} pending={createApp.isPending} error={createApp.error} />
          <DataTable ariaLabel="Apps" compact>
            <Table.Header>
              <Table.Column isRowHeader>Name</Table.Column>
              <Table.Column>Access</Table.Column>
              <Table.Column>Scope</Table.Column>
            </Table.Header>
            <Table.Body>
              {apps.map((app) => (
                <Table.Row className="click-row" id={app.id} key={app.id} onAction={() => setSelectedAppId(app.id)}>
                  <Table.Cell>{app.name}</Table.Cell>
                  <Table.Cell><StatusLabel value={app.role} /></Table.Cell>
                  <Table.Cell>{app.id === selectedAppId ? "Current" : "Switch"}</Table.Cell>
                </Table.Row>
              ))}
            </Table.Body>
          </DataTable>
        </Panel>
        <Panel title={selectedApp ? `${selectedApp.name} details` : "App details"}>
          {selectedApp?.permissions.includes("app.write") ? (
            <AppEditForm app={selectedApp} error={updateApp.error} pending={updateApp.isPending} onSubmit={(input) => updateApp.mutate(input)} />
          ) : selectedApp ? (
            <div className="permission-summary">
              <p className="muted">Your {titleize(selectedApp.role)} role has read-only app settings.</p>
              <PermissionList permissions={selectedApp.permissions} />
            </div>
          ) : (
            <EmptyState icon={<Boxes size={18} />} title="Create an app to continue" />
          )}
        </Panel>
        {selectedApp?.permissions.includes("members.manage") ? <AppMembersPanel app={selectedApp} /> : null}
        <Panel title="Sign-in methods">
          <div className="identity-list">
            <div className="identity-row">
              <KeyRound size={17} />
              <div><strong>Local password</strong><span>{identities.data?.has_local_password ? "Connected" : "Not configured"}</span></div>
              <StatusLabel value={identities.data?.has_local_password ? "active" : "disabled"} />
            </div>
            {(identities.data?.identities ?? []).map((identity) => (
              <div className="identity-row" key={identity.id}>
                <ShieldCheck size={17} />
                <div><strong>{identity.provider_name}</strong><span>{identity.email ?? "No email claim"}</span></div>
                <StatusLabel value={identity.email_verified ? "verified" : "unverified"} />
              </div>
            ))}
          </div>
          {setup.data?.oidc && !identities.data?.identities.length ? (
            <Link className="auth-provider-link" href={api.oidcStartUrl({ link: true, returnTo: "/settings" })}>
              Link {setup.data.oidc.name}
            </Link>
          ) : null}
          {identities.error ? <ErrorBlock error={identities.error} /> : null}
        </Panel>
        <Panel title="Data access">
          <div className="settings-actions">
            {selectedApp?.permissions.includes("export.run") ? <Link href={`${API_BASE_URL}/api/export/transactions.csv?app_id=${encodeURIComponent(selectedAppId)}`}>
              <Database size={16} />
              Export current app CSV
            </Link> : <span className="muted">Export permission is not included in your role.</span>}
          </div>
          <p className="muted">Webhook secrets are stored as hashes. Raw payload and export access follow the selected app role.</p>
        </Panel>
      </div>
    </Page>
  );
}

function AppMembersPanel({ app }: { app: AppRecord }) {
  const queryClient = useQueryClient();
  const members = useQuery({ queryKey: ["app-members", app.id], queryFn: () => api.appMembers(app.id) });
  const [email, setEmail] = useState("");
  const [role, setRole] = useState("viewer");
  const [createdInviteUrl, setCreatedInviteUrl] = useState<string | null>(null);
  const invite = useMutation({
    mutationFn: () => api.inviteAppMember(app.id, { email, role }),
    onSuccess: async (result) => {
      setCreatedInviteUrl(result.invitation.invite_url ?? null);
      setEmail("");
      await queryClient.invalidateQueries({ queryKey: ["app-members", app.id] });
    },
  });
  const updateRole = useMutation({
    mutationFn: ({ userId, nextRole }: { userId: string; nextRole: string }) => api.updateAppMember(app.id, userId, { role: nextRole }),
    onSuccess: async () => queryClient.invalidateQueries({ queryKey: ["app-members", app.id] }),
  });
  const remove = useMutation({
    mutationFn: (userId: string) => api.removeAppMember(app.id, userId),
    onSuccess: async () => queryClient.invalidateQueries({ queryKey: ["app-members", app.id] }),
  });
  const revoke = useMutation({
    mutationFn: (invitationId: string) => api.revokeAppInvitation(app.id, invitationId),
    onSuccess: async () => queryClient.invalidateQueries({ queryKey: ["app-members", app.id] }),
  });
  const roles = members.data?.roles ?? [];

  return (
    <div className="settings-span">
      <Panel title="People and access" actions={<StatusLabel value={`${members.data?.members.length ?? 0} members`} />}>
        <form
          className="invite-form"
          onSubmit={(event) => {
            event.preventDefault();
            setCreatedInviteUrl(null);
            invite.mutate();
          }}
        >
          <Field label="Email">
            <Input fullWidth value={email} onChange={(event) => setEmail(event.target.value)} type="email" placeholder="teammate@example.com" required variant="secondary" />
          </Field>
          <Field label="Role">
            <SelectControl ariaLabel="Invitation role" value={role} onChange={setRole} options={roles.map((item) => ({ value: item.key, label: item.name }))} />
          </Field>
          <Button isDisabled={invite.isPending || !email} size="sm" type="submit" variant="primary">
            <UserPlus size={16} /> Invite
          </Button>
        </form>
        {invite.error ? <ErrorBlock error={invite.error} /> : null}
        {createdInviteUrl ? <InviteLink value={createdInviteUrl} /> : null}
        <div className="role-guide">
          {roles.map((item) => (
            <div key={item.key}>
              <strong>{item.name}</strong>
              <span>{item.description}</span>
            </div>
          ))}
        </div>
        <DataTable ariaLabel="App members" compact>
          <Table.Header>
            <Table.Column isRowHeader>Person</Table.Column>
            <Table.Column>Role</Table.Column>
            <Table.Column>Granted through</Table.Column>
            <Table.Column>Action</Table.Column>
          </Table.Header>
          <Table.Body>
            {(members.data?.members ?? []).map((member) => (
              <Table.Row id={member.user_id} key={member.user_id}>
                <Table.Cell><MemberIdentity member={member} /></Table.Cell>
                <Table.Cell>
                  {member.access_origin === "membership" ? (
                    <MemberRoleSelect member={member} roles={roles} onChange={(nextRole) => updateRole.mutate({ userId: member.user_id, nextRole })} />
                  ) : <StatusLabel value={member.role} />}
                </Table.Cell>
                <Table.Cell>{titleize(member.access_origin)}</Table.Cell>
                <Table.Cell>
                  {member.access_origin === "membership" ? (
                    <Button aria-label={`Remove ${member.email}`} isIconOnly onPress={() => remove.mutate(member.user_id)} size="sm" variant="ghost"><Trash2 size={15} /></Button>
                  ) : null}
                </Table.Cell>
              </Table.Row>
            ))}
          </Table.Body>
        </DataTable>
        {(members.data?.invitations ?? []).length ? (
          <div className="pending-invitations">
            <h3>Pending invitations</h3>
            {members.data?.invitations.map((item) => (
              <div className="pending-invitation" key={item.id}>
                <div><strong>{item.email}</strong><span>{titleize(item.role)} · expires {formatDateTime(item.expires_at)}</span></div>
                <Button aria-label={`Revoke invitation for ${item.email}`} isIconOnly onPress={() => revoke.mutate(item.id)} size="sm" variant="ghost"><X size={15} /></Button>
              </div>
            ))}
          </div>
        ) : null}
        {members.error ? <ErrorBlock error={members.error} /> : null}
        {updateRole.error || remove.error || revoke.error ? <ErrorBlock error={updateRole.error ?? remove.error ?? revoke.error} /> : null}
      </Panel>
    </div>
  );
}

function MemberIdentity({ member }: { member: AppMemberRecord }) {
  return <div className="member-identity"><strong>{member.display_name ?? member.email}</strong><span>{member.email}</span></div>;
}

function MemberRoleSelect({ member, roles, onChange }: { member: AppMemberRecord; roles: AppRoleRecord[]; onChange: (role: string) => void }) {
  return <SelectControl ariaLabel={`Role for ${member.email}`} value={member.role} onChange={onChange} options={roles.map((item) => ({ value: item.key, label: item.name }))} />;
}

function InviteLink({ value }: { value: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <Alert status="success">
      <Alert.Indicator />
      <Alert.Content>
        <Alert.Title>Invitation created</Alert.Title>
        <Alert.Description>This link is shown once. Copy it before leaving this page.</Alert.Description>
        <code className="invite-url">{value}</code>
      </Alert.Content>
      <Button
        aria-label="Copy invitation link"
        isIconOnly
        onPress={async () => {
          await navigator.clipboard.writeText(value);
          setCopied(true);
        }}
        size="sm"
        variant="secondary"
      >{copied ? <Check size={16} /> : <Copy size={16} />}</Button>
    </Alert>
  );
}

function PermissionList({ permissions }: { permissions: string[] }) {
  return <div className="permission-list">{permissions.map((permission) => <Chip key={permission} size="sm" variant="soft">{permission}</Chip>)}</div>;
}

function SourceForm({
  allowCredentials,
  apps,
  initialAppId,
  pending,
  error,
  onSubmit,
}: {
  allowCredentials: boolean;
  apps: AppRecord[];
  initialAppId?: string;
  pending: boolean;
  error: unknown;
  onSubmit: (input: { source_type: string; name: string; app_id?: string | null; credentials?: Record<string, unknown> }) => void;
}) {
  const [sourceType, setSourceType] = useState("revenuecat");
  const [name, setName] = useState("RevenueCat Production");
  const [appId, setAppId] = useState("");
  const [secret, setSecret] = useState("");
  const [catchUpText, setCatchUpText] = useState("");
  const [appAppleId, setAppAppleId] = useState("");
  const [appStoreEnvironment, setAppStoreEnvironment] = useState("both");
  const [appStoreIssuerId, setAppStoreIssuerId] = useState("");
  const [appStoreKeyId, setAppStoreKeyId] = useState("");
  const [appStorePrivateKey, setAppStorePrivateKey] = useState("");
  const [googlePushServiceAccountEmail, setGooglePushServiceAccountEmail] = useState("");
  const [googleSubscription, setGoogleSubscription] = useState("");
  const [googleServiceAccount, setGoogleServiceAccount] = useState<Record<string, unknown> | null>(null);
  const [googleServiceAccountFileName, setGoogleServiceAccountFileName] = useState("");
  const [parseError, setParseError] = useState<string | null>(null);
  const sourceApp = apps.find((app) => app.id === appId);

  useEffect(() => {
    if (initialAppId && apps.some((app) => app.id === initialAppId)) {
      setAppId(initialAppId);
    } else if (!appId && apps[0]) {
      setAppId(apps[0].id);
    }
  }, [appId, apps, initialAppId]);

  function submit(event: FormEvent) {
    event.preventDefault();
    setParseError(null);
    let credentials: Record<string, unknown> = {};
    if (sourceType === "app_store") {
      if (!sourceApp?.apple_bundle_id) {
        setParseError("Add an Apple bundle ID to this app before connecting App Store.");
        return;
      }
      if (allowCredentials) {
        if (appStoreEnvironment !== "sandbox" && !appAppleId.trim()) {
          setParseError("Apple ID is required for production notifications.");
          return;
        }
        credentials.environment = appStoreEnvironment;
        if (appAppleId.trim()) credentials.app_apple_id = appAppleId.trim();
        const historyValues = [appStoreIssuerId, appStoreKeyId, appStorePrivateKey].map((value) => value.trim());
        if (historyValues.some(Boolean) && !historyValues.every(Boolean)) {
          setParseError("Issuer ID, Key ID, and private key are all required to enable test notifications and recovery.");
          return;
        }
        if (historyValues.every(Boolean)) {
          credentials.issuer_id = historyValues[0];
          credentials.key_id = historyValues[1];
          credentials.private_key = historyValues[2];
        }
      }
    } else if (sourceType === "google_play") {
      if (!sourceApp?.google_package_name) {
        setParseError("Add a Google package name to this app before connecting Google Play.");
        return;
      }
      if (allowCredentials) {
        if (!googlePushServiceAccountEmail.trim()) {
          setParseError("Push authentication service account email is required.");
          return;
        }
        if (googleSubscription.trim() && !/^projects\/[^/]+\/subscriptions\/[^/]+$/.test(googleSubscription.trim())) {
          setParseError("Subscription path must use projects/PROJECT_ID/subscriptions/SUBSCRIPTION_ID.");
          return;
        }
        credentials.pubsub_service_account_email = googlePushServiceAccountEmail.trim();
        if (googleSubscription.trim()) credentials.pubsub_subscription = googleSubscription.trim();
        if (googleServiceAccount) credentials.service_account_json = googleServiceAccount;
      }
    } else if (allowCredentials && supportsCatchUp(sourceType) && catchUpText.trim()) {
      try {
        const parsed = JSON.parse(catchUpText) as unknown;
        if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
          setParseError("Catch-up JSON must be an object.");
          return;
        }
        credentials = { ...(parsed as Record<string, unknown>) };
      } catch {
        setParseError("Catch-up JSON must be valid JSON.");
        return;
      }
    }
    if (allowCredentials && !["app_store", "google_play"].includes(sourceType) && secret) credentials.webhook_secret = secret;
    const payload = Object.keys(credentials).length ? credentials : undefined;
    onSubmit({ source_type: sourceType, name, app_id: appId || null, credentials: payload });
  }

  return (
    <form className="stack" onSubmit={submit}>
      <Field label="Source type">
        <SelectControl
          ariaLabel="Source type"
          value={sourceType}
          onChange={(next) => {
            setSourceType(next);
            setName(defaultSourceName(next));
            setCatchUpText("");
            setAppAppleId("");
            setAppStoreEnvironment("both");
            setAppStoreIssuerId("");
            setAppStoreKeyId("");
            setAppStorePrivateKey("");
            setGooglePushServiceAccountEmail("");
            setGoogleSubscription("");
            setGoogleServiceAccount(null);
            setGoogleServiceAccountFileName("");
            setParseError(null);
          }}
          options={[
            { value: "revenuecat", label: "RevenueCat" },
            { value: "custom_api", label: "Custom API" },
            { value: "app_store", label: "App Store" },
            { value: "google_play", label: "Google Play" },
            { value: "stripe", label: "Stripe" },
            { value: "paddle", label: "Paddle" },
          ]}
        />
      </Field>
      <Field label="Name">
        <Input fullWidth value={name} onChange={(event) => setName(event.target.value)} required variant="secondary" />
      </Field>
      <Field label="App">
        <SelectControl
          ariaLabel="Source app"
          value={appId}
          onChange={(next) => {
            setAppId(next);
            if (sourceType === "app_store") setAppAppleId("");
          }}
          options={[
            ...apps.map((app) => ({ value: app.id, label: app.name })),
          ]}
        />
      </Field>
      {allowCredentials && !["app_store", "google_play"].includes(sourceType) ? <Field label="Webhook secret">
        <Input fullWidth value={secret} onChange={(event) => setSecret(event.target.value)} placeholder="Optional shared secret" variant="secondary" />
      </Field> : null}
      {sourceType === "app_store" ? (
        allowCredentials ? (
          <AppStoreCredentialsFields
            appAppleId={appAppleId}
            bundleId={sourceApp?.apple_bundle_id ?? null}
            environment={appStoreEnvironment}
            issuerId={appStoreIssuerId}
            keyId={appStoreKeyId}
            onAppAppleIdChange={setAppAppleId}
            onEnvironmentChange={setAppStoreEnvironment}
            onIssuerIdChange={setAppStoreIssuerId}
            onKeyIdChange={setAppStoreKeyId}
            onPrivateKeyChange={setAppStorePrivateKey}
            privateKey={appStorePrivateKey}
          />
        ) : (
          <p className="muted">An owner or admin must finish the App Store identity settings after this source is created.</p>
        )
      ) : sourceType === "google_play" ? (
        allowCredentials ? (
          <GooglePlayCredentialsFields
            packageName={sourceApp?.google_package_name ?? null}
            pushServiceAccountEmail={googlePushServiceAccountEmail}
            serviceAccount={googleServiceAccount}
            serviceAccountFileName={googleServiceAccountFileName}
            subscription={googleSubscription}
            onPushServiceAccountEmailChange={setGooglePushServiceAccountEmail}
            onServiceAccountChange={(value, fileName) => {
              setGoogleServiceAccount(value);
              setGoogleServiceAccountFileName(fileName);
            }}
            onSubscriptionChange={setGoogleSubscription}
          />
        ) : (
          <p className="muted">An owner or admin must finish the Google Play push authentication settings after this source is created.</p>
        )
      ) : allowCredentials && supportsCatchUp(sourceType) ? (
        <Field label="Source credentials and verification JSON">
          <TextArea
            fullWidth
            value={catchUpText}
            onChange={(event) => setCatchUpText(event.target.value)}
            placeholder={catchUpTemplate(sourceType)}
            rows={7}
            variant="secondary"
          />
        </Field>
      ) : null}
      {parseError ? <p className="form-error">{parseError}</p> : null}
      {error ? <ErrorBlock error={error} /> : null}
      <Button isDisabled={pending || !name || !appId} size="sm" type="submit" variant="primary">
        <Plus size={16} />
        Add source
      </Button>
    </form>
  );
}

function AppStoreCredentialsFields({
  appAppleId,
  bundleId,
  environment,
  issuerId,
  keyId,
  privateKey,
  catchUpConfigured = false,
  onAppAppleIdChange,
  onEnvironmentChange,
  onIssuerIdChange,
  onKeyIdChange,
  onPrivateKeyChange,
}: {
  appAppleId: string;
  bundleId?: string | null;
  environment: string;
  issuerId: string;
  keyId: string;
  privateKey: string;
  catchUpConfigured?: boolean;
  onAppAppleIdChange: (value: string) => void;
  onEnvironmentChange: (value: string) => void;
  onIssuerIdChange: (value: string) => void;
  onKeyIdChange: (value: string) => void;
  onPrivateKeyChange: (value: string) => void;
}) {
  return (
    <div className="source-form-section">
      <div className="source-form-heading">
        <strong>Live notifications</strong>
        <span>Apple root certificates and the webhook endpoint are managed automatically.</span>
      </div>
      <div className={`derived-field${bundleId ? "" : " derived-field--missing"}`}>
        <span>Bundle ID</span>
        <code>{bundleId ?? "Missing from app settings"}</code>
        <small>{bundleId ? "From the selected app" : "Add it in Apps before continuing"}</small>
      </div>
      <div className="source-form-grid">
        <Field label={environment === "sandbox" ? "Apple ID (optional in Sandbox)" : "Apple ID"}>
          <Input
            fullWidth
            inputMode="numeric"
            value={appAppleId}
            onChange={(event) => onAppAppleIdChange(event.target.value.replace(/\D/g, ""))}
            placeholder="1234567890"
            required={environment !== "sandbox"}
            variant="secondary"
          />
          <span className="field-help">The numeric Apple ID shown in App Store Connect.</span>
        </Field>
        <Field label="Environment">
          <SelectControl
            ariaLabel="App Store environment"
            value={environment}
            onChange={onEnvironmentChange}
            options={[
              { value: "both", label: "Production and Sandbox" },
              { value: "production", label: "Production only" },
              { value: "sandbox", label: "Sandbox only" },
            ]}
          />
        </Field>
      </div>
      <details className="advanced-settings">
        <summary>
          <span>Test notifications and recovery</span>
          <small>{catchUpConfigured ? "Configured · leave blank to keep current key" : "Optional"}</small>
        </summary>
        <div className="advanced-settings__content">
          <p>Needed only for one-click tests and Catch up. Create an In-App Purchase key in App Store Connect.</p>
          <div className="source-form-grid">
            <Field label="Issuer ID">
              <Input fullWidth value={issuerId} onChange={(event) => onIssuerIdChange(event.target.value)} placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx" variant="secondary" />
            </Field>
            <Field label="Key ID">
              <Input fullWidth value={keyId} onChange={(event) => onKeyIdChange(event.target.value)} placeholder="ABC123DEFG" variant="secondary" />
            </Field>
          </div>
          <Field label="Private key (.p8 contents)">
            <TextArea
              fullWidth
              value={privateKey}
              onChange={(event) => onPrivateKeyChange(event.target.value)}
              placeholder="-----BEGIN PRIVATE KEY-----"
              rows={5}
              variant="secondary"
            />
          </Field>
        </div>
      </details>
    </div>
  );
}

function GooglePlayCredentialsFields({
  packageName,
  webhookAudience,
  pushServiceAccountEmail,
  subscription,
  serviceAccount,
  serviceAccountFileName,
  serviceAccountConfigured = false,
  sharedSecretConfigured = false,
  catchUpConfigured = false,
  purchaseVerificationConfigured = false,
  onPushServiceAccountEmailChange,
  onSubscriptionChange,
  onServiceAccountChange,
}: {
  packageName?: string | null;
  webhookAudience?: string | null;
  pushServiceAccountEmail: string;
  subscription: string;
  serviceAccount: Record<string, unknown> | null;
  serviceAccountFileName: string;
  serviceAccountConfigured?: boolean;
  sharedSecretConfigured?: boolean;
  catchUpConfigured?: boolean;
  purchaseVerificationConfigured?: boolean;
  onPushServiceAccountEmailChange: (value: string) => void;
  onSubscriptionChange: (value: string) => void;
  onServiceAccountChange: (value: Record<string, unknown> | null, fileName: string) => void;
}) {
  const [fileError, setFileError] = useState<string | null>(null);

  async function selectServiceAccountFile(file?: File) {
    setFileError(null);
    if (!file) {
      onServiceAccountChange(null, "");
      return;
    }
    try {
      const parsed = JSON.parse(await file.text()) as unknown;
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) throw new Error("invalid object");
      const serviceAccount = parsed as Record<string, unknown>;
      const clientEmail = typeof serviceAccount.client_email === "string" ? serviceAccount.client_email.trim() : "";
      const privateKey = typeof serviceAccount.private_key === "string" ? serviceAccount.private_key.trim() : "";
      if (!clientEmail || !privateKey) throw new Error("missing fields");
      onServiceAccountChange(serviceAccount, file.name);
      if (!pushServiceAccountEmail.trim()) onPushServiceAccountEmailChange(clientEmail);
    } catch {
      onServiceAccountChange(null, "");
      setFileError("Choose a Google service account JSON file containing client_email and private_key.");
    }
  }

  return (
    <div className="source-form-section">
      <div className="source-form-heading">
        <strong>Live notifications</strong>
        <span>The package name and OIDC audience are managed automatically.</span>
      </div>
      <div className={`derived-field${packageName ? "" : " derived-field--missing"}`}>
        <span>Package name</span>
        <code>{packageName ?? "Missing from app settings"}</code>
        <small>{packageName ? "From the selected app" : "Add it in Apps before continuing"}</small>
      </div>
      <div className="derived-field">
        <span>OIDC audience</span>
        <code>{webhookAudience ?? "Generated with the webhook URL"}</code>
        <small>Use this value in the Pub/Sub push subscription</small>
      </div>
      <Field label={sharedSecretConfigured ? "Push authentication service account email (optional)" : "Push authentication service account email"}>
        <Input
          fullWidth
          type="email"
          value={pushServiceAccountEmail}
          onChange={(event) => onPushServiceAccountEmailChange(event.target.value)}
          placeholder="pubsub-push@example.iam.gserviceaccount.com"
          required={!sharedSecretConfigured}
          variant="secondary"
        />
        <span className="field-help">
          {sharedSecretConfigured
            ? "This source currently uses an edge-provided shared secret. Add an email to switch to Pub/Sub OIDC."
            : "The service account selected under Pub/Sub push authentication."}
        </span>
      </Field>
      <details className="advanced-settings">
        <summary>
          <span>Purchase verification and recovery</span>
          <small>{catchUpConfigured ? "Recovery configured" : purchaseVerificationConfigured ? "Purchase verification configured" : "Optional"}</small>
        </summary>
        <div className="advanced-settings__content">
          <p>Upload a service account key to verify purchases with Android Publisher API. Add a subscription path to also enable Catch up.</p>
          <Field label="Service account key (.json)">
            <input
              accept=".json,application/json"
              aria-label="Google service account key file"
              className="file-input"
              onChange={(event) => void selectServiceAccountFile(event.target.files?.[0])}
              type="file"
            />
            <span className="field-help">
              {serviceAccountFileName
                ? `${serviceAccountFileName} selected`
                : serviceAccountConfigured
                  ? "A key is already configured. Choose a file only to replace it."
                  : "The key is encrypted before it is stored."}
            </span>
          </Field>
          {fileError ? <p className="form-error">{fileError}</p> : null}
          <Field label="Pub/Sub subscription path">
            <Input
              fullWidth
              value={subscription}
              onChange={(event) => onSubscriptionChange(event.target.value)}
              placeholder="projects/PROJECT_ID/subscriptions/SUBSCRIPTION_ID"
              variant="secondary"
            />
            <span className="field-help">Optional. Required only for pulling retained RTDN messages.</span>
          </Field>
          {serviceAccount ? <span className="credential-ready">Service account file is ready to save.</span> : null}
        </div>
      </details>
    </div>
  );
}

function SourceCard({
  source,
  onTest,
  onSendAppStoreTest,
  onCatchUp,
  onConfigureCatchUp,
  appStoreTestPending,
  appStoreTestResult,
  appStoreTestError,
  configurePending,
  configureError,
}: {
  source: DataSourceRecord;
  onTest?: () => void;
  onSendAppStoreTest?: (environment: AppStoreTestEnvironment) => void;
  onCatchUp?: () => void;
  onConfigureCatchUp?: (credentials: Record<string, unknown>) => void;
  appStoreTestPending?: boolean;
  appStoreTestResult?: AppStoreTestNotification;
  appStoreTestError?: unknown;
  configurePending?: boolean;
  configureError?: unknown;
}) {
  const [copied, setCopied] = useState(false);
  const [editingCredentials, setEditingCredentials] = useState(false);
  const [credentialsText, setCredentialsText] = useState("");
  const [credentialsParseError, setCredentialsParseError] = useState<string | null>(null);
  const [appAppleId, setAppAppleId] = useState(source.configuration.app_apple_id ?? "");
  const [appStoreEnvironment, setAppStoreEnvironment] = useState(source.configuration.environment ?? "both");
  const [appStoreIssuerId, setAppStoreIssuerId] = useState("");
  const [appStoreKeyId, setAppStoreKeyId] = useState("");
  const [appStorePrivateKey, setAppStorePrivateKey] = useState("");
  const [appStoreTestEnvironment, setAppStoreTestEnvironment] = useState<AppStoreTestEnvironment>(
    source.configuration.environment === "production" ? "production" : "sandbox",
  );
  const [googlePushServiceAccountEmail, setGooglePushServiceAccountEmail] = useState(source.configuration.pubsub_service_account_email ?? "");
  const [googleSubscription, setGoogleSubscription] = useState(source.configuration.pubsub_subscription ?? "");
  const [googleServiceAccount, setGoogleServiceAccount] = useState<Record<string, unknown> | null>(null);
  const [googleServiceAccountFileName, setGoogleServiceAccountFileName] = useState("");

  function saveCredentials(event: FormEvent) {
    event.preventDefault();
    setCredentialsParseError(null);
    if (source.source_type === "app_store") {
      if (appStoreEnvironment !== "sandbox" && !appAppleId.trim() && !source.credential_keys.includes("app_apple_id")) {
        setCredentialsParseError("Apple ID is required for production notifications.");
        return;
      }
      const historyValues = [appStoreIssuerId, appStoreKeyId, appStorePrivateKey].map((value) => value.trim());
      if (historyValues.some(Boolean) && !historyValues.every(Boolean)) {
        setCredentialsParseError("Issuer ID, Key ID, and private key are all required to enable test notifications and recovery.");
        return;
      }
      const credentials: Record<string, unknown> = { environment: appStoreEnvironment };
      if (appAppleId.trim()) credentials.app_apple_id = appAppleId.trim();
      if (historyValues.every(Boolean)) {
        credentials.issuer_id = historyValues[0];
        credentials.key_id = historyValues[1];
        credentials.private_key = historyValues[2];
      }
      onConfigureCatchUp?.(credentials);
      return;
    }
    if (source.source_type === "google_play") {
      if (!googlePushServiceAccountEmail.trim() && source.verification_mode !== "shared_secret") {
        setCredentialsParseError("Push authentication service account email is required.");
        return;
      }
      if (googleSubscription.trim() && !/^projects\/[^/]+\/subscriptions\/[^/]+$/.test(googleSubscription.trim())) {
        setCredentialsParseError("Subscription path must use projects/PROJECT_ID/subscriptions/SUBSCRIPTION_ID.");
        return;
      }
      const credentials: Record<string, unknown> = {};
      if (googlePushServiceAccountEmail.trim()) credentials.pubsub_service_account_email = googlePushServiceAccountEmail.trim();
      if (googleSubscription.trim()) credentials.pubsub_subscription = googleSubscription.trim();
      if (googleServiceAccount) credentials.service_account_json = googleServiceAccount;
      onConfigureCatchUp?.(credentials);
      return;
    }
    if (!credentialsText.trim()) {
      setCredentialsParseError("Credentials JSON is required.");
      return;
    }
    try {
      const parsed = JSON.parse(credentialsText) as unknown;
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
        setCredentialsParseError("Credentials JSON must be an object.");
        return;
      }
      onConfigureCatchUp?.(parsed as Record<string, unknown>);
    } catch {
      setCredentialsParseError("Credentials JSON must be valid JSON.");
    }
  }

  useEffect(() => {
    if (!configurePending && !configureError) {
      setEditingCredentials(false);
      setCredentialsText("");
      setAppAppleId(source.configuration.app_apple_id ?? "");
      setAppStoreEnvironment(source.configuration.environment ?? "both");
      setAppStoreIssuerId("");
      setAppStoreKeyId("");
      setAppStorePrivateKey("");
      setGooglePushServiceAccountEmail(source.configuration.pubsub_service_account_email ?? "");
      setGoogleSubscription(source.configuration.pubsub_subscription ?? "");
      setGoogleServiceAccount(null);
      setGoogleServiceAccountFileName("");
      setCredentialsParseError(null);
    }
  }, [
    configureError,
    configurePending,
    source.configuration.app_apple_id,
    source.configuration.environment,
    source.configuration.pubsub_service_account_email,
    source.configuration.pubsub_subscription,
    source.has_credentials,
  ]);

  useEffect(() => {
    setAppStoreTestEnvironment(
      source.configuration.environment === "production" ? "production" : "sandbox",
    );
  }, [source.configuration.environment]);

  const appStoreTestOptions = source.configuration.environment === "production"
    ? [{ value: "production", label: "Production" }]
    : source.configuration.environment === "sandbox"
      ? [{ value: "sandbox", label: "Sandbox" }]
      : [
          { value: "sandbox", label: "Sandbox" },
          { value: "production", label: "Production" },
        ];

  return (
    <section className="source-card">
      <header className="source-card-head">
        <div>
          <strong>{source.name}</strong>
          <span>{titleize(source.source_type)} · {source.app_name ?? "No app selected"}</span>
          <span>Webhook verification: {source.verification_mode === "missing" ? "Not configured" : titleize(source.verification_mode)}</span>
        </div>
        <StatusLabel value={source.status} />
      </header>
      <div className="source-card__content">
        <div className="webhook-row">
          <code>{source.webhook_url}</code>
          <Button
            aria-label="Copy webhook URL"
            isIconOnly
            onPress={async () => {
              await navigator.clipboard.writeText(source.webhook_url);
              setCopied(true);
              window.setTimeout(() => setCopied(false), 1200);
            }}
            size="sm"
            variant="secondary"
          >
            {copied ? <Check size={16} /> : <Copy size={16} />}
          </Button>
        </div>
        <ul className="check-list">
          {source.setup_checklist.map((item) => (
            <li key={item.key}>
              {item.done ? (
                <Check size={15} />
              ) : item.optional ? (
                <Minus className="optional-icon" size={15} />
              ) : (
                <X className="incomplete-icon" size={15} />
              )}
              <span>{item.label}</span>
            </li>
          ))}
        </ul>
        {source.source_type === "app_store" ? (
          <div className="source-test">
            <div className="source-test__head">
              <div>
                <strong>Apple test notification</strong>
                <span>Apple sends a signed TEST event to this source's webhook URL.</span>
              </div>
              <div className="source-test__controls">
                <SelectControl
                  ariaLabel="Apple test environment"
                  className="source-test__environment"
                  value={appStoreTestEnvironment}
                  onChange={(value) => setAppStoreTestEnvironment(value as AppStoreTestEnvironment)}
                  options={appStoreTestOptions}
                />
                <Button
                  isDisabled={!source.catch_up_configured || !onSendAppStoreTest || appStoreTestPending}
                  onPress={() => onSendAppStoreTest?.(appStoreTestEnvironment)}
                  size="sm"
                  variant="secondary"
                >
                  {appStoreTestPending ? <RefreshCw size={16} /> : <Send size={16} />}
                  {appStoreTestPending ? "Requesting…" : "Send test"}
                </Button>
              </div>
            </div>
            {!source.catch_up_configured ? (
              <span className="field-help">Add an In-App Purchase key under Configure to enable one-click tests.</span>
            ) : !onSendAppStoreTest ? (
              <span className="field-help">Your role can view this source but cannot send test notifications.</span>
            ) : null}
            {appStoreTestResult ? (
              <Alert className="source-test__result" status="success">
                <Alert.Indicator><Check size={16} /></Alert.Indicator>
                <Alert.Content>
                  <Alert.Title>Test requested</Alert.Title>
                  <Alert.Description>
                    Apple accepted the {titleize(appStoreTestResult.environment)} request. The setup checklist updates after the callback arrives.
                  </Alert.Description>
                  <code title={appStoreTestResult.test_notification_token}>{appStoreTestResult.test_notification_token}</code>
                </Alert.Content>
              </Alert>
            ) : null}
            {appStoreTestError ? <ErrorBlock error={appStoreTestError} /> : null}
          </div>
        ) : null}
        <div className="card-actions">
          {onTest ? <Button onPress={onTest} size="sm" variant="secondary">
            <RefreshCw size={16} />
            Test
          </Button> : null}
          {onConfigureCatchUp ? (
            <Button
              onPress={() => {
                setEditingCredentials((current) => !current);
                setCredentialsParseError(null);
              }}
              size="sm"
              variant="secondary"
            >
              <KeyRound size={16} />
              Configure
            </Button>
          ) : null}
          {onCatchUp ? (
            <Button onPress={onCatchUp} size="sm" variant="secondary">
              <CloudDownload size={16} />
              Catch up
            </Button>
          ) : null}
        </div>
        {editingCredentials && onConfigureCatchUp ? (
          <form className="source-credentials-form" onSubmit={saveCredentials}>
            {source.source_type === "app_store" ? (
              <AppStoreCredentialsFields
                appAppleId={appAppleId}
                bundleId={source.configuration.bundle_id}
                catchUpConfigured={source.catch_up_configured}
                environment={appStoreEnvironment}
                issuerId={appStoreIssuerId}
                keyId={appStoreKeyId}
                onAppAppleIdChange={setAppAppleId}
                onEnvironmentChange={setAppStoreEnvironment}
                onIssuerIdChange={setAppStoreIssuerId}
                onKeyIdChange={setAppStoreKeyId}
                onPrivateKeyChange={setAppStorePrivateKey}
                privateKey={appStorePrivateKey}
              />
            ) : source.source_type === "google_play" ? (
              <GooglePlayCredentialsFields
                packageName={source.configuration.package_name}
                webhookAudience={source.configuration.pubsub_oidc_audience}
                pushServiceAccountEmail={googlePushServiceAccountEmail}
                serviceAccount={googleServiceAccount}
                serviceAccountConfigured={source.credential_keys.includes("service_account_json")}
                serviceAccountFileName={googleServiceAccountFileName}
                sharedSecretConfigured={source.verification_mode === "shared_secret"}
                subscription={googleSubscription}
                catchUpConfigured={source.catch_up_configured}
                purchaseVerificationConfigured={source.purchase_verification_configured}
                onPushServiceAccountEmailChange={setGooglePushServiceAccountEmail}
                onServiceAccountChange={(value, fileName) => {
                  setGoogleServiceAccount(value);
                  setGoogleServiceAccountFileName(fileName);
                }}
                onSubscriptionChange={setGoogleSubscription}
              />
            ) : (
              <Field label="Source credentials and verification JSON">
                <TextArea
                  fullWidth
                  value={credentialsText}
                  onChange={(event) => setCredentialsText(event.target.value)}
                  placeholder={catchUpTemplate(source.source_type)}
                  rows={7}
                  variant="secondary"
                />
              </Field>
            )}
            {credentialsParseError ? <p className="form-error">{credentialsParseError}</p> : null}
            {configureError ? <ErrorBlock error={configureError} /> : null}
            <div className="card-actions">
              <Button onPress={() => setEditingCredentials(false)} size="sm" type="button" variant="secondary">
                Cancel
              </Button>
              <Button isDisabled={configurePending} size="sm" type="submit" variant="primary">
                <Check size={16} />
                Save
              </Button>
            </div>
          </form>
        ) : null}
        {source.last_error ? <p className="form-error">{source.last_error}</p> : null}
      </div>
    </section>
  );
}

function AppForm({
  pending,
  error,
  onSubmit,
}: {
  pending: boolean;
  error: unknown;
  onSubmit: (input: { name: string; apple_bundle_id?: string; google_package_name?: string; default_currency?: string }) => void;
}) {
  const [name, setName] = useState("");
  const [appleBundleId, setAppleBundleId] = useState("");
  const [googlePackageName, setGooglePackageName] = useState("");
  const [defaultCurrency, setDefaultCurrency] = useState("USD");
  return (
    <form
      className="inline-form"
      onSubmit={(event) => {
        event.preventDefault();
        onSubmit({ name, apple_bundle_id: appleBundleId || undefined, google_package_name: googlePackageName || undefined, default_currency: defaultCurrency || undefined });
        setName("");
        setAppleBundleId("");
        setGooglePackageName("");
      }}
    >
      <Input fullWidth value={name} onChange={(event) => setName(event.target.value)} placeholder="App name" required variant="secondary" />
      <Input fullWidth value={appleBundleId} onChange={(event) => setAppleBundleId(event.target.value)} placeholder="Apple bundle id" variant="secondary" />
      <Input fullWidth value={googlePackageName} onChange={(event) => setGooglePackageName(event.target.value)} placeholder="Google package" variant="secondary" />
      <Input fullWidth value={defaultCurrency} onChange={(event) => setDefaultCurrency(event.target.value.toUpperCase())} placeholder="USD" className="short-input" variant="secondary" />
      <Button isDisabled={pending} size="sm" type="submit" variant="primary">
        <Plus size={16} />
        Add
      </Button>
      {error ? <ErrorBlock error={error} /> : null}
    </form>
  );
}

function AppEditForm({
  app,
  pending,
  error,
  onSubmit,
}: {
  app: AppRecord;
  pending: boolean;
  error: unknown;
  onSubmit: (input: { name: string; apple_bundle_id?: string; google_package_name?: string; default_currency?: string }) => void;
}) {
  const [name, setName] = useState(app.name);
  const [appleBundleId, setAppleBundleId] = useState(app.apple_bundle_id ?? "");
  const [googlePackageName, setGooglePackageName] = useState(app.google_package_name ?? "");
  const [defaultCurrency, setDefaultCurrency] = useState(app.default_currency ?? "USD");
  useEffect(() => {
    setName(app.name);
    setAppleBundleId(app.apple_bundle_id ?? "");
    setGooglePackageName(app.google_package_name ?? "");
    setDefaultCurrency(app.default_currency ?? "USD");
  }, [app]);
  return (
    <form
      className="stack"
      onSubmit={(event) => {
        event.preventDefault();
        onSubmit({
          name,
          apple_bundle_id: appleBundleId || undefined,
          google_package_name: googlePackageName || undefined,
          default_currency: defaultCurrency || undefined,
        });
      }}
    >
      <Field label="Name"><Input fullWidth value={name} onChange={(event) => setName(event.target.value)} required variant="secondary" /></Field>
      <Field label="Apple bundle ID"><Input fullWidth value={appleBundleId} onChange={(event) => setAppleBundleId(event.target.value)} variant="secondary" /></Field>
      <Field label="Google package"><Input fullWidth value={googlePackageName} onChange={(event) => setGooglePackageName(event.target.value)} variant="secondary" /></Field>
      <Field label="Default currency"><Input fullWidth value={defaultCurrency} onChange={(event) => setDefaultCurrency(event.target.value.toUpperCase())} variant="secondary" /></Field>
      <PermissionList permissions={app.permissions} />
      {error ? <ErrorBlock error={error} /> : null}
      <Button isDisabled={pending || !name} size="sm" type="submit" variant="primary"><Check size={16} />Save changes</Button>
    </form>
  );
}

function TransactionTable({
  transactions,
  compact = false,
  onSelect,
}: {
  transactions: TransactionRecord[];
  compact?: boolean;
  onSelect?: (id: string) => void;
}) {
  if (!transactions.length) return <EmptyState icon={<Receipt size={18} />} title="No transactions yet" />;
  return (
    <DataTable ariaLabel="Transactions" compact={compact}>
      <Table.Header>
        <Table.Column isRowHeader>Time</Table.Column>
        <Table.Column>Product</Table.Column>
        <Table.Column>Source</Table.Column>
        {!compact ? <Table.Column>Transaction</Table.Column> : null}
        <Table.Column>Amount</Table.Column>
        {!compact ? <Table.Column>Environment</Table.Column> : null}
        <Table.Column>Status</Table.Column>
        {!compact ? <Table.Column>Country</Table.Column> : null}
      </Table.Header>
      <Table.Body>
        {transactions.map((transaction) => (
          <Table.Row className={onSelect ? "click-row" : undefined} id={transaction.id} key={transaction.id} onAction={onSelect ? () => onSelect(transaction.id) : undefined}>
            <Table.Cell>{formatDateTime(transaction.purchase_time)}</Table.Cell>
            <Table.Cell>{transaction.logical_product_name ?? transaction.source_product_name ?? "Unmapped"}</Table.Cell>
            <Table.Cell>{titleize(transaction.source_type)} · {titleize(transaction.platform)}</Table.Cell>
            {!compact ? <Table.Cell className="mono-cell">{transaction.transaction_key}</Table.Cell> : null}
            <Table.Cell>{formatMoney(transaction.amount_minor, transaction.currency)}</Table.Cell>
            {!compact ? <Table.Cell><StatusLabel value={transaction.environment} /></Table.Cell> : null}
            <Table.Cell><StatusLabel value={transaction.status} /></Table.Cell>
            {!compact ? <Table.Cell>{transaction.country ?? "—"}</Table.Cell> : null}
          </Table.Row>
        ))}
      </Table.Body>
    </DataTable>
  );
}

function ProductTable({ products }: { products: LogicalProductRecord[] }) {
  if (!products.length) return <EmptyState icon={<Boxes size={18} />} title="No confirmed products" />;
  return (
    <DataTable ariaLabel="Confirmed products">
      <Table.Header>
        <Table.Column isRowHeader>Product</Table.Column>
        <Table.Column>Kind</Table.Column>
        <Table.Column>Period</Table.Column>
        <Table.Column>Category</Table.Column>
        <Table.Column>Sources</Table.Column>
      </Table.Header>
      <Table.Body>
        {products.map((product) => (
          <Table.Row id={product.id} key={product.id}>
            <Table.Cell>{product.display_name}</Table.Cell>
            <Table.Cell>{titleize(product.product_kind)}</Table.Cell>
            <Table.Cell>{titleize(product.billing_period)}</Table.Cell>
            <Table.Cell>{product.reporting_category ?? "—"}</Table.Cell>
            <Table.Cell>{product.source_products.map((source) => source.external_product_id ?? source.display_name ?? source.id).join(", ") || "—"}</Table.Cell>
          </Table.Row>
        ))}
      </Table.Body>
    </DataTable>
  );
}

function DataTable({ ariaLabel, children, compact = false }: { ariaLabel: string; children: ReactNode; compact?: boolean }) {
  return (
    <Table className={cx("data-table", compact && "compact-table")} variant="secondary">
      <Table.ScrollContainer>
        <Table.Content aria-label={ariaLabel}>{children}</Table.Content>
      </Table.ScrollContainer>
    </Table>
  );
}

function cx(...classes: Array<string | false | null | undefined>) {
  return classes.filter(Boolean).join(" ");
}

type SelectControlOption = {
  label: string;
  value: string;
};

const emptySelectValue = "__revtern_empty__";

function selectKey(value: string) {
  return value === "" ? emptySelectValue : value;
}

function valueFromSelectKey(key: unknown) {
  const value = String(key);
  return value === emptySelectValue ? "" : value;
}

function SelectControl({
  ariaLabel,
  className,
  onChange,
  options,
  value,
}: {
  ariaLabel: string;
  className?: string;
  onChange: (value: string) => void;
  options: SelectControlOption[];
  value: string;
}) {
  const selectedKey = options.some((option) => option.value === value) ? selectKey(value) : undefined;
  return (
    <Select
      aria-label={ariaLabel}
      className={cx("select-control", className)}
      onSelectionChange={(key) => {
        if (key !== null) onChange(valueFromSelectKey(key));
      }}
      selectedKey={selectedKey}
      variant="secondary"
    >
      <Select.Trigger>
        <Select.Value />
        <Select.Indicator />
      </Select.Trigger>
      <Select.Popover>
        <ListBox>
          {options.map((option) => (
            <ListBox.Item id={selectKey(option.value)} key={selectKey(option.value)}>
              {option.label}
            </ListBox.Item>
          ))}
        </ListBox>
      </Select.Popover>
    </Select>
  );
}

function Page({ title, actions, children }: { title: string; actions?: ReactNode; children: ReactNode }) {
  return (
    <div className="page">
      <header className="page-header">
        <Typography.Heading level={1} className="page-title">{title}</Typography.Heading>
        {actions ? <div className="page-actions">{actions}</div> : null}
      </header>
      <div className="page-content">{children}</div>
    </div>
  );
}

function AppLink({ children, to }: { children: ReactNode; to: string }) {
  return (
    <RouterNavLink className="link" to={to}>
      {children}
    </RouterNavLink>
  );
}

function Panel({ title, actions, children }: { title: string; actions?: ReactNode; children: ReactNode }) {
  return (
    <Card className="panel" variant="default">
      <Card.Header className="panel-head">
        <Card.Title className="panel-title">{title}</Card.Title>
        {actions ? <div className="panel-actions">{actions}</div> : null}
      </Card.Header>
      <Card.Content className="panel-content">{children}</Card.Content>
    </Card>
  );
}

function FilterBar({
  filters,
  compact,
  onChange,
}: {
  filters: Record<string, string>;
  compact?: boolean;
  onChange: (next: Record<string, string>) => void;
}) {
  const products = useQuery({ queryKey: ["logical-products", filters.app_id], queryFn: () => api.logicalProducts({ app_id: filters.app_id }) });
  return (
    <div className={compact ? "filter-bar compact-filter" : "filter-bar"}>
      <Input aria-label="From date" className="date-input" type="date" value={filters.from} onChange={(event) => onChange({ ...filters, from: event.target.value })} variant="secondary" />
      <Input aria-label="To date" className="date-input" type="date" value={filters.to} onChange={(event) => onChange({ ...filters, to: event.target.value })} variant="secondary" />
      <SelectControl
        ariaLabel="Dashboard platform"
        value={filters.platform}
        onChange={(platform) => onChange({ ...filters, platform })}
        options={[
          { value: "all", label: "All platforms" },
          { value: "ios", label: "iOS" },
          { value: "android", label: "Android" },
          { value: "web", label: "Web" },
        ]}
      />
      <SelectControl
        ariaLabel="Dashboard product"
        value={filters.logical_product_id}
        onChange={(logical_product_id) => onChange({ ...filters, logical_product_id })}
        options={[
          { value: "all", label: "All products" },
          ...(products.data?.logical_products ?? []).map((product) => ({ value: product.id, label: product.display_name })),
        ]}
      />
      <Input aria-label="Currency" className="short-input" value={filters.currency} onChange={(event) => onChange({ ...filters, currency: event.target.value.toUpperCase() })} variant="secondary" />
    </div>
  );
}

function useDashboardFilters() {
  const { selectedAppId } = useAppScope();
  const range = useMemo(last30Days, []);
  const [filters, setFilters] = useState<Record<string, string>>({
    ...range,
    app_id: selectedAppId,
    platform: "all",
    logical_product_id: "all",
    currency: "USD",
  });
  useEffect(() => {
    setFilters((current) => current.app_id === selectedAppId ? current : { ...current, app_id: selectedAppId });
  }, [selectedAppId]);
  return [filters, setFilters] as const;
}

function Segmented({ value, options, onChange }: { value: string; options: string[]; onChange: (value: string) => void }) {
  return (
    <ToggleButtonGroup
      aria-label="Segmented options"
      className="segmented"
      disallowEmptySelection
      onSelectionChange={(keys) => {
        const [next] = [...keys];
        if (next) onChange(String(next));
      }}
      selectedKeys={[value]}
      selectionMode="single"
      size="sm"
    >
      {options.map((option) => (
        <ToggleButton id={option} key={option} size="sm" variant="ghost">
          {titleize(option)}
        </ToggleButton>
      ))}
    </ToggleButtonGroup>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="field">
      <span className="field-label">{label}</span>
      {children}
    </label>
  );
}

function StatusLabel({ value }: { value: string }) {
  const normalized = value.toLowerCase();
  return (
    <Chip color={statusColor(normalized)} size="sm" variant="soft">
      {titleize(value)}
    </Chip>
  );
}

function statusClass(value: string) {
  if (["active", "paid", "renewed", "completed", "processed", "live", "verified", "production", "reconciled"].includes(value)) return "good";
  if (["failed", "dead", "error", "refunded", "revoked"].includes(value)) return "bad";
  if (["estimated", "warning", "unmapped", "waiting_for_events", "queued", "running", "stored", "sandbox", "test", "unknown"].includes(value)) return "warn";
  return "neutral";
}

function statusColor(value: string) {
  switch (statusClass(value)) {
    case "good":
      return "success";
    case "bad":
      return "danger";
    case "warn":
      return "warning";
    default:
      return "default";
  }
}

function ErrorBlock({ error }: { error: unknown }) {
  const message = error instanceof ApiError ? error.message : error instanceof Error ? error.message : "Request failed";
  return (
    <Alert status="danger">
      <Alert.Indicator><AlertTriangle size={16} /></Alert.Indicator>
      <Alert.Content>
        <Alert.Description>{message}</Alert.Description>
      </Alert.Content>
    </Alert>
  );
}

function EmptyState({ icon, title, action }: { icon: ReactNode; title: string; action?: ReactNode }) {
  return (
    <HeroEmptyState>
      {icon}
      <span>{title}</span>
      {action}
    </HeroEmptyState>
  );
}

function ChartFrame({ children }: { children: ReactNode }) {
  return <div className="chart-frame">{children}</div>;
}

function JsonDrawer({ title, value, onClose }: { title: string; value: unknown; onClose: () => void }) {
  const drawer = useOverlayState({
    isOpen: true,
    onOpenChange: (isOpen) => {
      if (!isOpen) onClose();
    },
  });

  return (
    <Drawer state={drawer}>
      <Drawer.Backdrop isDismissable variant="opaque">
        <Drawer.Content placement="right">
          <Drawer.Dialog aria-label={title}>
            <Drawer.Header>
              <Drawer.Heading>{title}</Drawer.Heading>
              <Drawer.CloseTrigger />
            </Drawer.Header>
            <Drawer.Body className="json-drawer-body">
              <pre className="json-drawer-pre">{JSON.stringify(value, null, 2)}</pre>
            </Drawer.Body>
          </Drawer.Dialog>
        </Drawer.Content>
      </Drawer.Backdrop>
    </Drawer>
  );
}

type EvidenceEvent = {
  id: string;
  event_type: string;
  environment: string;
  occurred_at: string;
  raw_event_id: string;
  amount_minor?: number | null;
  currency?: string | null;
  warnings: string[];
};

function EvidenceDrawer({
  title,
  subtitle,
  events,
  error,
  loading,
  onClose,
  subscription,
}: {
  title: string;
  subtitle?: string;
  events: EvidenceEvent[];
  error: unknown;
  loading: boolean;
  onClose: () => void;
  subscription?: SubscriptionRecord;
}) {
  const drawer = useOverlayState({
    isOpen: true,
    onOpenChange: (isOpen) => {
      if (!isOpen) onClose();
    },
  });

  return (
    <Drawer state={drawer}>
      <Drawer.Backdrop isDismissable variant="opaque">
        <Drawer.Content placement="right">
          <Drawer.Dialog aria-label={title}>
            <Drawer.Header>
              <div className="evidence-heading">
                <Drawer.Heading>{title}</Drawer.Heading>
                {subtitle ? <span>{subtitle}</span> : null}
              </div>
              <Drawer.CloseTrigger />
            </Drawer.Header>
            <Drawer.Body className="evidence-drawer-body">
              {loading ? <div className="drawer-loading">Loading source evidence…</div> : null}
              {error ? <ErrorBlock error={error} /> : null}
              {subscription ? (
                <dl className="evidence-summary">
                  <div><dt>Current status</dt><dd><StatusLabel value={subscription.status} /></dd></div>
                  <div><dt>Period start</dt><dd>{formatDateTime(subscription.current_period_start)}</dd></div>
                  <div><dt>Period end</dt><dd>{formatDateTime(subscription.current_period_end)}</dd></div>
                  <div><dt>Renewal</dt><dd>{subscription.will_renew ? "Will renew" : "Will not renew"}</dd></div>
                </dl>
              ) : null}
              {!loading && !error && !events.length ? (
                <EmptyState icon={<Database size={18} />} title="No linked evidence events" />
              ) : null}
              {events.length ? (
                <ol className="evidence-timeline">
                  {events.map((event) => (
                    <li key={event.id}>
                      <div className="evidence-event-head">
                        <strong>{titleize(event.event_type)}</strong>
                        <StatusLabel value={event.environment} />
                      </div>
                      <span>{formatDateTime(event.occurred_at)}</span>
                      {event.amount_minor != null ? <span>{formatMoney(event.amount_minor, event.currency ?? "USD")}</span> : null}
                      <code>{event.raw_event_id}</code>
                      {event.warnings.length ? (
                        <ul>
                          {event.warnings.map((warning) => <li key={warning}>{warning}</li>)}
                        </ul>
                      ) : null}
                    </li>
                  ))}
                </ol>
              ) : null}
            </Drawer.Body>
          </Drawer.Dialog>
        </Drawer.Content>
      </Drawer.Backdrop>
    </Drawer>
  );
}

function updateDraft(setDrafts: (updater: (drafts: CatalogDraftGroup[]) => CatalogDraftGroup[]) => void, index: number, patch: Partial<CatalogDraftGroup>) {
  setDrafts((drafts) => drafts.map((draft, draftIndex) => (draftIndex === index ? { ...draft, ...patch } : draft)));
}

function defaultSourceName(sourceType: string) {
  switch (sourceType) {
    case "app_store":
      return "App Store Connect";
    case "google_play":
      return "Google Play";
    case "custom_api":
      return "Custom API";
    case "stripe":
      return "Stripe";
    case "paddle":
      return "Paddle";
    default:
      return "RevenueCat Production";
  }
}

function supportsCatchUp(sourceType: string) {
  return sourceType === "app_store" || sourceType === "google_play";
}

function catchUpTemplate(sourceType: string) {
  if (sourceType === "app_store") {
    return JSON.stringify(
      {
        issuer_id: "App Store Connect issuer id",
        key_id: "App Store Connect key id",
        private_key: "-----BEGIN PRIVATE KEY-----\\n...\\n-----END PRIVATE KEY-----",
        bundle_id: "com.example.app",
        app_apple_id: "1234567890",
        environment: "both",
        apple_root_ca_pem: "-----BEGIN CERTIFICATE-----\\nApple Root CA downloaded from apple.com/certificateauthority\\n-----END CERTIFICATE-----"
      },
      null,
      2,
    );
  }
  if (sourceType === "google_play") {
    return JSON.stringify(
      {
        pubsub_subscription: "projects/PROJECT_ID/subscriptions/SUBSCRIPTION_ID",
        pubsub_oidc_audience: "https://revtern.example.com/webhooks/google-play/SOURCE_ID",
        pubsub_service_account_email: "pubsub-push@example.iam.gserviceaccount.com",
        service_account_json: {
          client_email: "revtern-google-play@example.iam.gserviceaccount.com",
          private_key: "-----BEGIN PRIVATE KEY-----\\n...\\n-----END PRIVATE KEY-----",
          token_uri: "https://oauth2.googleapis.com/token"
        }
      },
      null,
      2,
    );
  }
  return "";
}
