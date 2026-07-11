import { ApiError, type CatalogConfirmation } from "@revtern/api-client";
import type {
  AppRecord,
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
  Plus,
  RadioTower,
  Receipt,
  RefreshCw,
  Search,
  Settings,
  ShieldCheck,
  SquareStack,
  TrendingUp,
  X,
} from "lucide-react";
import { FormEvent, ReactNode, useEffect, useMemo, useState } from "react";
import { NavLink as RouterNavLink, Navigate, Route, Routes } from "react-router-dom";
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
  { to: "/", label: "Overview", icon: LayoutDashboard },
  { to: "/revenue", label: "Revenue", icon: LineChartIcon },
  { to: "/transactions", label: "Transactions", icon: Receipt },
  { to: "/subscriptions", label: "Subscriptions", icon: SquareStack },
  { to: "/products", label: "Products", icon: Boxes },
  { to: "/events", label: "Events", icon: Database },
  { to: "/sources", label: "Sources", icon: RadioTower },
  { to: "/reconciliation", label: "Reconciliation", icon: AlertTriangle },
  { to: "/jobs", label: "Jobs", icon: ListChecks },
  { to: "/settings", label: "Settings", icon: Settings },
];

export default function App() {
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
  if (me.error) return <LoginScreen authMode={setup.data?.auth_mode ?? "single_user"} />;
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

function LoginScreen({ authMode }: { authMode: string }) {
  const queryClient = useQueryClient();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const mutation = useMutation({
    mutationFn: () => api.login({ email, password }),
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
        <h1>Sign in</h1>
        {authMode === "reverse_proxy" ? <p className="muted">Reverse proxy mode is enabled; the trusted user header was not present.</p> : null}
        <Field label="Email">
          <Input fullWidth value={email} onChange={(event) => setEmail(event.target.value)} type="email" autoFocus required variant="secondary" />
        </Field>
        <Field label="Password">
          <Input fullWidth value={password} onChange={(event) => setPassword(event.target.value)} type="password" required variant="secondary" />
        </Field>
        {mutation.error ? <ErrorBlock error={mutation.error} /> : null}
        <Button isDisabled={mutation.isPending} size="sm" type="submit" variant="primary">
          <Check size={16} />
          Sign in
        </Button>
      </form>
    </AuthFrame>
  );
}

function AppShell({ me }: { me: { user: { email: string; role: string }; workspace: { name: string } } }) {
  const queryClient = useQueryClient();
  const logout = useMutation({
    mutationFn: () => api.logout(),
    onSuccess: async () => {
      await queryClient.invalidateQueries();
    },
  });

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand-row sidebar-brand">
          <span className="brand-mark">R</span>
          <div>
            <strong>Revtern</strong>
            <span>{me.workspace.name}</span>
          </div>
        </div>
        <nav className="nav-list">
          {navItems.map((item) => {
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
  );
}

function OverviewPage() {
  const queryClient = useQueryClient();
  const [filters, setFilters] = useDashboardFilters();
  const overview = useQuery({ queryKey: ["overview", filters], queryFn: () => api.overview(filters) });
  const series = useQuery({ queryKey: ["revenue-series", filters], queryFn: () => api.revenueTimeseries(filters) });
  const transactions = useQuery({ queryKey: ["transactions", "recent", filters], queryFn: () => api.transactions({ ...filters }), placeholderData: (previous) => previous });
  const sources = useQuery({ queryKey: ["data-sources"], queryFn: () => api.dataSources() });
  const sourceProducts = useQuery({ queryKey: ["source-products", "overview"], queryFn: () => api.sourceProducts() });
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
  const apps = useQuery({ queryKey: ["apps"], queryFn: () => api.apps() });
  const sourceProducts = useQuery({ queryKey: ["source-products"], queryFn: () => api.sourceProducts() });
  const logicalProducts = useQuery({ queryKey: ["logical-products"], queryFn: () => api.logicalProducts() });
  const firstAppId = apps.data?.apps[0]?.id ?? "";
  const [selectedAppId, setSelectedAppId] = useState(firstAppId);
  const [drafts, setDrafts] = useState<CatalogDraftGroup[]>([]);
  const [ignored, setIgnored] = useState<Set<string>>(new Set());
  const unmapped = useMemo(
    () => (sourceProducts.data?.source_products ?? []).filter((product) => product.mapping_state === "unmapped"),
    [sourceProducts.data],
  );

  useEffect(() => {
    if (!selectedAppId && firstAppId) setSelectedAppId(firstAppId);
  }, [firstAppId, selectedAppId]);

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
          <SelectControl
            ariaLabel="Catalog app"
            value={selectedAppId}
            onChange={setSelectedAppId}
            options={(apps.data?.apps ?? []).map((app) => ({ value: app.id, label: app.name }))}
          />
          <Button onPress={() => setDrafts(buildCatalogDraft(unmapped))} size="sm" variant="secondary">
            <RefreshCw size={16} />
            Regenerate draft
          </Button>
          <Button
            isDisabled={!selectedAppId || confirm.isPending || (!drafts.length && !ignored.size)}
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
  const [tab, setTab] = useState<"raw" | "normalized">("raw");
  const [q, setQ] = useState("");
  const raw = useQuery({ queryKey: ["raw-events", q], queryFn: () => api.rawEvents({ q }), enabled: tab === "raw" });
  const normalized = useQuery({ queryKey: ["normalized-events"], queryFn: () => api.normalizedEvents(), enabled: tab === "normalized" });
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
  const apps = useQuery({ queryKey: ["apps"], queryFn: () => api.apps() });
  const sources = useQuery({ queryKey: ["data-sources"], queryFn: () => api.dataSources() });
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
          <SourceForm apps={apps.data?.apps ?? []} onSubmit={(input) => create.mutate(input)} pending={create.isPending} error={create.error} />
        </Panel>
        <Panel title="Connected sources">
          <div className="source-card-list">
            {(sources.data?.data_sources ?? []).map((source) => (
              <SourceCard
                key={source.id}
                source={source}
                onTest={() => test.mutate(source.id)}
                onConfigureCatchUp={supportsCatchUp(source.source_type)
                  ? (credentials) => updateCredentials.mutate({ id: source.id, credentials })
                  : undefined}
                onCatchUp={supportsCatchUp(source.source_type) && source.has_credentials ? () => catchUp.mutate(source.id) : undefined}
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
  const overview = useQuery({ queryKey: ["overview", "reconciliation"], queryFn: () => api.overview({}) });
  const sourceProducts = useQuery({ queryKey: ["source-products"], queryFn: () => api.sourceProducts() });
  const jobs = useQuery({ queryKey: ["jobs"], queryFn: () => api.jobs() });
  const rawFailed = useQuery({ queryKey: ["raw-events", "failed"], queryFn: () => api.rawEvents({ processing_status: "failed" }) });
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
  const jobs = useQuery({ queryKey: ["jobs"], queryFn: () => api.jobs(), refetchInterval: 10_000 });
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
  const apps = useQuery({ queryKey: ["apps"], queryFn: () => api.apps() });
  const createApp = useMutation({
    mutationFn: (input: { name: string; apple_bundle_id?: string; google_package_name?: string; default_currency?: string }) => api.createApp(input),
    onSuccess: async () => queryClient.invalidateQueries({ queryKey: ["apps"] }),
  });
  return (
    <Page title="Settings">
      <div className="two-column align-start">
        <Panel title="Apps">
          <AppForm onSubmit={(input) => createApp.mutate(input)} pending={createApp.isPending} error={createApp.error} />
          <DataTable ariaLabel="Apps" compact>
            <Table.Header>
              <Table.Column isRowHeader>Name</Table.Column>
              <Table.Column>Apple bundle</Table.Column>
              <Table.Column>Google package</Table.Column>
              <Table.Column>Currency</Table.Column>
            </Table.Header>
            <Table.Body>
              {(apps.data?.apps ?? []).map((app) => (
                <Table.Row id={app.id} key={app.id}>
                  <Table.Cell>{app.name}</Table.Cell>
                  <Table.Cell className="mono-cell">{app.apple_bundle_id ?? "—"}</Table.Cell>
                  <Table.Cell className="mono-cell">{app.google_package_name ?? "—"}</Table.Cell>
                  <Table.Cell>{app.default_currency ?? "—"}</Table.Cell>
                </Table.Row>
              ))}
            </Table.Body>
          </DataTable>
        </Panel>
        <Panel title="Local data">
          <div className="settings-actions">
            <Link href={`${API_BASE_URL}/api/export/transactions.csv`}>
              <Database size={16} />
              Export transactions CSV
            </Link>
          </div>
          <p className="muted">Webhook secrets are stored as hashes. Raw payload access is limited to the owner session.</p>
        </Panel>
      </div>
    </Page>
  );
}

function SourceForm({
  apps,
  pending,
  error,
  onSubmit,
}: {
  apps: AppRecord[];
  pending: boolean;
  error: unknown;
  onSubmit: (input: { source_type: string; name: string; app_id?: string | null; credentials?: Record<string, unknown> }) => void;
}) {
  const [sourceType, setSourceType] = useState("revenuecat");
  const [name, setName] = useState("RevenueCat Production");
  const [appId, setAppId] = useState("");
  const [secret, setSecret] = useState("");
  const [catchUpText, setCatchUpText] = useState("");
  const [parseError, setParseError] = useState<string | null>(null);

  useEffect(() => {
    if (!appId && apps[0]) setAppId(apps[0].id);
  }, [appId, apps]);

  function submit(event: FormEvent) {
    event.preventDefault();
    setParseError(null);
    let credentials: Record<string, unknown> = {};
    if (supportsCatchUp(sourceType) && catchUpText.trim()) {
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
    if (secret) credentials.webhook_secret = secret;
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
          onChange={setAppId}
          options={[
            { value: "", label: "No app" },
            ...apps.map((app) => ({ value: app.id, label: app.name })),
          ]}
        />
      </Field>
      <Field label="Webhook secret">
        <Input fullWidth value={secret} onChange={(event) => setSecret(event.target.value)} placeholder="Optional shared secret" variant="secondary" />
      </Field>
      {supportsCatchUp(sourceType) ? (
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
      <Button isDisabled={pending || !name} size="sm" type="submit" variant="primary">
        <Plus size={16} />
        Add source
      </Button>
    </form>
  );
}

function SourceCard({
  source,
  onTest,
  onCatchUp,
  onConfigureCatchUp,
  configurePending,
  configureError,
}: {
  source: DataSourceRecord;
  onTest: () => void;
  onCatchUp?: () => void;
  onConfigureCatchUp?: (credentials: Record<string, unknown>) => void;
  configurePending?: boolean;
  configureError?: unknown;
}) {
  const [copied, setCopied] = useState(false);
  const [editingCredentials, setEditingCredentials] = useState(false);
  const [credentialsText, setCredentialsText] = useState("");
  const [credentialsParseError, setCredentialsParseError] = useState<string | null>(null);

  function saveCredentials(event: FormEvent) {
    event.preventDefault();
    setCredentialsParseError(null);
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
      setCredentialsParseError(null);
    }
  }, [configureError, configurePending, source.has_credentials]);

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
              {item.done ? <Check size={15} /> : <X size={15} />}
              <span>{item.label}</span>
            </li>
          ))}
        </ul>
        <div className="card-actions">
          <Button onPress={onTest} size="sm" variant="secondary">
            <RefreshCw size={16} />
            Test
          </Button>
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
  const apps = useQuery({ queryKey: ["apps"], queryFn: () => api.apps() });
  const products = useQuery({ queryKey: ["logical-products"], queryFn: () => api.logicalProducts() });
  return (
    <div className={compact ? "filter-bar compact-filter" : "filter-bar"}>
      <Input aria-label="From date" className="date-input" type="date" value={filters.from} onChange={(event) => onChange({ ...filters, from: event.target.value })} variant="secondary" />
      <Input aria-label="To date" className="date-input" type="date" value={filters.to} onChange={(event) => onChange({ ...filters, to: event.target.value })} variant="secondary" />
      <SelectControl
        ariaLabel="Dashboard app"
        value={filters.app_id}
        onChange={(app_id) => onChange({ ...filters, app_id })}
        options={[
          { value: "all", label: "All apps" },
          ...(apps.data?.apps ?? []).map((app) => ({ value: app.id, label: app.name })),
        ]}
      />
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
  const range = useMemo(last30Days, []);
  return useState<Record<string, string>>({
    ...range,
    app_id: "all",
    platform: "all",
    logical_product_id: "all",
    currency: "USD",
  });
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
