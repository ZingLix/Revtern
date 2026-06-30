import { ApiError, type CatalogConfirmation } from "@revtern/api-client";
import type {
  AppRecord,
  DataSourceRecord,
  JobRecord,
  LogicalProductRecord,
  RawEventRecord,
  SourceProductRecord,
  TransactionRecord,
} from "@revtern/types";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  AlertTriangle,
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
  SquareStack,
  X,
} from "lucide-react";
import { FormEvent, ReactNode, useEffect, useMemo, useState } from "react";
import { NavLink, Navigate, Route, Routes } from "react-router-dom";
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
import { formatDate, formatDateTime, formatMoney, formatNumber, formatPercent, last30Days, titleize } from "./lib/format";

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
      <section className="auth-panel">
        <div className="brand-row">
          <span className="brand-mark">R</span>
          <strong>Revtern</strong>
        </div>
        {children}
      </section>
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
          <input value={email} onChange={(event) => setEmail(event.target.value)} type="email" autoFocus required />
        </Field>
        <Field label="Password">
          <input value={password} onChange={(event) => setPassword(event.target.value)} type="password" minLength={8} required />
        </Field>
        <Field label="Workspace">
          <input value={workspaceName} onChange={(event) => setWorkspaceName(event.target.value)} required />
        </Field>
        {mutation.error ? <ErrorBlock error={mutation.error} /> : null}
        <button className="primary-button" disabled={mutation.isPending}>
          <Check size={16} />
          Create owner
        </button>
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
          <input value={email} onChange={(event) => setEmail(event.target.value)} type="email" autoFocus required />
        </Field>
        <Field label="Password">
          <input value={password} onChange={(event) => setPassword(event.target.value)} type="password" required />
        </Field>
        {mutation.error ? <ErrorBlock error={mutation.error} /> : null}
        <button className="primary-button" disabled={mutation.isPending}>
          <Check size={16} />
          Sign in
        </button>
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
              <NavLink key={item.to} to={item.to} end={item.to === "/"} className={({ isActive }) => (isActive ? "nav-link active" : "nav-link")}>
                <Icon size={17} />
                {item.label}
              </NavLink>
            );
          })}
        </nav>
        <div className="sidebar-footer">
          <span>{me.user.email}</span>
          <button className="icon-text-button" onClick={() => logout.mutate()} disabled={logout.isPending}>
            <LogOut size={16} />
            Log out
          </button>
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
  const transactions = useQuery({ queryKey: ["transactions", "recent"], queryFn: () => api.transactions({ ...filters }), placeholderData: (previous) => previous });
  const sources = useQuery({ queryKey: ["data-sources"], queryFn: () => api.dataSources() });
  const seed = useMutation({
    mutationFn: () => api.seedDemo(),
    onSuccess: async () => {
      await queryClient.invalidateQueries();
    },
  });

  return (
    <Page title="Overview" actions={<FilterBar filters={filters} onChange={setFilters} />}>
      {overview.error ? <ErrorBlock error={overview.error} /> : null}
      <section className="metric-grid">
        <MetricCard label="Gross revenue" value={formatMoney(overview.data?.metrics.gross_revenue_minor.value, overview.data?.currency)} state={overview.data?.metrics.gross_revenue_minor.trust_state} />
        <MetricCard label="Net revenue" value={formatMoney(overview.data?.metrics.net_revenue_minor.value, overview.data?.currency)} state={overview.data?.metrics.net_revenue_minor.trust_state} />
        <MetricCard label="Active subscriptions" value={formatNumber(overview.data?.metrics.active_subscriptions.value)} state={overview.data?.metrics.active_subscriptions.trust_state} />
        <MetricCard label="New subscriptions" value={formatNumber(overview.data?.metrics.new_subscriptions.value)} state={overview.data?.metrics.new_subscriptions.trust_state} />
        <MetricCard label="Renewals" value={formatNumber(overview.data?.metrics.renewals.value)} state={overview.data?.metrics.renewals.trust_state} />
        <MetricCard label="Refunds" value={formatMoney(overview.data?.metrics.refund_amount_minor.value, overview.data?.currency)} state={overview.data?.metrics.refund_amount_minor.trust_state} />
        <MetricCard label="Refund rate" value={formatPercent(overview.data?.metrics.refund_rate.value)} state={overview.data?.metrics.refund_rate.trust_state} />
        <MetricCard label="Churned" value={formatNumber(overview.data?.metrics.churned_subscriptions.value)} state={overview.data?.metrics.churned_subscriptions.trust_state} />
      </section>
      {overview.data?.warnings.length ? (
        <Panel title="Metric notes">
          <ul className="issue-list">
            {overview.data.warnings.map((warning) => (
              <li key={warning}>
                <AlertTriangle size={16} />
                <span>{warning}</span>
              </li>
            ))}
          </ul>
        </Panel>
      ) : null}
      <div className="two-column">
        <Panel title="Revenue trend">
          <ChartFrame>
            <ResponsiveContainer width="100%" height={260}>
              <LineChart data={series.data?.series ?? []}>
                <CartesianGrid strokeDasharray="3 3" vertical={false} />
                <XAxis dataKey="date" tickFormatter={formatDate} />
                <YAxis tickFormatter={(value) => `$${Number(value) / 100}`} width={56} />
                <Tooltip formatter={(value) => formatMoney(Number(value), overview.data?.currency)} labelFormatter={(value) => formatDate(String(value))} />
                <Legend />
                <Line dataKey="gross_revenue_minor" name="Gross" stroke="#0f766e" strokeWidth={2} dot={false} />
                <Line dataKey="net_revenue_minor" name="Net" stroke="#334155" strokeWidth={2} dot={false} />
              </LineChart>
            </ResponsiveContainer>
          </ChartFrame>
        </Panel>
        <Panel title="Setup state">
          <div className="source-health-list">
            {(sources.data?.data_sources ?? []).map((source) => (
              <div className="source-health" key={source.id}>
                <div>
                  <strong>{source.name}</strong>
                  <span>{titleize(source.source_type)} · {source.app_name ?? "No app"}</span>
                </div>
                <StatusLabel value={source.status} />
              </div>
            ))}
            {!sources.data?.data_sources.length ? (
              <EmptyState
                icon={<RadioTower size={18} />}
                title="No source connected"
                action={<NavLink className="secondary-button" to="/sources"><Plus size={16} /> Add source</NavLink>}
              />
            ) : null}
          </div>
        </Panel>
      </div>
      <Panel
        title="Recent transactions"
        actions={
          !transactions.data?.transactions.length ? (
            <button className="secondary-button" onClick={() => seed.mutate()} disabled={seed.isPending}>
              <Plus size={16} />
              Seed demo
            </button>
          ) : null
        }
      >
        <TransactionTable transactions={(transactions.data?.transactions ?? []).slice(0, 8)} compact />
      </Panel>
    </Page>
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
              <YAxis tickFormatter={(value) => `$${Number(value) / 100}`} />
              <Tooltip formatter={(value) => formatMoney(Number(value), filters.currency)} labelFormatter={(value) => formatDate(String(value))} />
              <Legend />
              <Bar dataKey="gross_revenue_minor" name="Gross" fill="#0f766e" />
              <Bar dataKey="refund_amount_minor" name="Refunds" fill="#dc2626" />
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
        <table className="data-table">
          <thead>
            <tr>
              <th>{titleize(by)}</th>
              <th>Gross</th>
              <th>Refunds</th>
              <th>Transactions</th>
            </tr>
          </thead>
          <tbody>
            {(breakdown.data?.items ?? []).map((item) => (
              <tr key={item.label}>
                <td>{item.label}</td>
                <td>{formatMoney(item.gross_revenue_minor, filters.currency)}</td>
                <td>{formatMoney(item.refund_amount_minor, filters.currency)}</td>
                <td>{formatNumber(item.transaction_count)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </Panel>
    </Page>
  );
}

function TransactionsPage() {
  const [filters, setFilters] = useDashboardFilters();
  const [status, setStatus] = useState("all");
  const transactions = useQuery({
    queryKey: ["transactions", filters, status],
    queryFn: () => api.transactions({ ...filters, status }),
  });
  return (
    <Page
      title="Transactions"
      actions={
        <div className="toolbar">
          <FilterBar filters={filters} onChange={setFilters} compact />
          <select value={status} onChange={(event) => setStatus(event.target.value)}>
            <option value="all">All statuses</option>
            <option value="paid">Paid</option>
            <option value="renewed">Renewed</option>
            <option value="refunded">Refunded</option>
            <option value="revoked">Revoked</option>
          </select>
        </div>
      }
    >
      <Panel title="Ledger">
        {transactions.error ? <ErrorBlock error={transactions.error} /> : null}
        <TransactionTable transactions={transactions.data?.transactions ?? []} />
      </Panel>
    </Page>
  );
}

function SubscriptionsPage() {
  const [status, setStatus] = useState("all");
  const subscriptions = useQuery({ queryKey: ["subscriptions", status], queryFn: () => api.subscriptions({ status }) });
  const subSeries = useQuery({ queryKey: ["subscription-series"], queryFn: () => api.subscriptionTimeseries({}) });
  return (
    <Page
      title="Subscriptions"
      actions={
        <select value={status} onChange={(event) => setStatus(event.target.value)}>
          <option value="all">All statuses</option>
          <option value="trialing">Trialing</option>
          <option value="active">Active</option>
          <option value="cancelled_active">Cancelled active</option>
          <option value="billing_retry">Billing retry</option>
          <option value="expired">Expired</option>
        </select>
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
              <Line dataKey="new_subscription_count" name="New" stroke="#0f766e" strokeWidth={2} dot={false} />
              <Line dataKey="renewal_count" name="Renewals" stroke="#475569" strokeWidth={2} dot={false} />
              <Line dataKey="cancel_count" name="Cancels" stroke="#b45309" strokeWidth={2} dot={false} />
            </LineChart>
          </ResponsiveContainer>
        </ChartFrame>
      </Panel>
      <Panel title="Current subscriptions">
        <table className="data-table">
          <thead>
            <tr>
              <th>Subscription</th>
              <th>Product</th>
              <th>Platform</th>
              <th>Status</th>
              <th>Started</th>
              <th>Renewal</th>
            </tr>
          </thead>
          <tbody>
            {(subscriptions.data?.subscriptions ?? []).map((subscription) => (
              <tr key={subscription.id}>
                <td className="mono-cell">{subscription.subscription_key}</td>
                <td>{subscription.logical_product_name ?? subscription.source_product_name ?? "Unmapped"}</td>
                <td>{titleize(subscription.platform)}</td>
                <td><StatusLabel value={subscription.status} /></td>
                <td>{formatDateTime(subscription.started_at)}</td>
                <td>{subscription.will_renew ? "Will renew" : "Won't renew"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </Panel>
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
        <div className="toolbar">
          <select value={selectedAppId} onChange={(event) => setSelectedAppId(event.target.value)}>
            {apps.data?.apps.map((app) => <option key={app.id} value={app.id}>{app.name}</option>)}
          </select>
          <button className="secondary-button" onClick={() => setDrafts(buildCatalogDraft(unmapped))}>
            <RefreshCw size={16} />
            Regenerate draft
          </button>
          <button className="primary-button" onClick={() => confirm.mutate()} disabled={!selectedAppId || confirm.isPending || (!drafts.length && !ignored.size)}>
            <Check size={16} />
            Confirm catalog
          </button>
        </div>
      }
    >
      {confirm.error ? <ErrorBlock error={confirm.error} /> : null}
      <Panel title="Catalog draft">
        {!unmapped.length ? (
          <EmptyState icon={<Boxes size={18} />} title="No unmapped source products" />
        ) : (
          <div className="draft-list">
            {drafts.map((draft, index) => (
              <div className="draft-item" key={draft.client_id}>
                <div className="draft-form-grid">
                  <Field label="Product name">
                    <input value={draft.display_name} onChange={(event) => updateDraft(setDrafts, index, { display_name: event.target.value })} />
                  </Field>
                  <Field label="Kind">
                    <select value={draft.product_kind} onChange={(event) => updateDraft(setDrafts, index, { product_kind: event.target.value })}>
                      <option value="subscription">Subscription</option>
                      <option value="consumable">Consumable</option>
                      <option value="non_consumable">Non-consumable</option>
                      <option value="lifetime">Lifetime</option>
                      <option value="unknown">Unknown</option>
                    </select>
                  </Field>
                  <Field label="Period">
                    <select value={draft.billing_period} onChange={(event) => updateDraft(setDrafts, index, { billing_period: event.target.value })}>
                      <option value="weekly">Weekly</option>
                      <option value="monthly">Monthly</option>
                      <option value="annual">Annual</option>
                      <option value="lifetime">Lifetime</option>
                      <option value="none">None</option>
                      <option value="unknown">Unknown</option>
                    </select>
                  </Field>
                  <Field label="Category">
                    <input value={draft.reporting_category} onChange={(event) => updateDraft(setDrafts, index, { reporting_category: event.target.value })} />
                  </Field>
                </div>
                <span className="muted">{draft.reason}</span>
                <div className="source-product-list">
                  {draft.source_product_ids.map((id) => {
                    const product = unmapped.find((item) => item.id === id);
                    if (!product) return null;
                    return (
                      <label className="source-product-row" key={id}>
                        <input
                          type="checkbox"
                          checked={!ignored.has(id)}
                          onChange={(event) => {
                            const next = new Set(ignored);
                            if (event.target.checked) next.delete(id);
                            else next.add(id);
                            setIgnored(next);
                          }}
                        />
                        <span>{product.external_product_id ?? product.display_name ?? id}</span>
                        <span>{titleize(product.source_type)} · {titleize(product.product_kind)} · {titleize(product.billing_period)}</span>
                      </label>
                    );
                  })}
                </div>
              </div>
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
        <div className="toolbar">
          <Segmented value={tab} onChange={(value) => setTab(value as "raw" | "normalized")} options={["raw", "normalized"]} />
          <label className="search-box">
            <Search size={16} />
            <input value={q} onChange={(event) => setQ(event.target.value)} placeholder="Search raw payloads" disabled={tab !== "raw"} />
          </label>
        </div>
      }
    >
      {tab === "raw" ? (
        <Panel title="Raw events">
          <table className="data-table">
            <thead>
              <tr>
                <th>Received</th>
                <th>Source</th>
                <th>Event</th>
                <th>Product</th>
                <th>Status</th>
                <th>Signature</th>
              </tr>
            </thead>
            <tbody>
              {(raw.data?.raw_events ?? []).map((event) => (
                <tr key={event.id} onClick={() => setSelected(event)} className="click-row">
                  <td>{formatDateTime(event.received_at)}</td>
                  <td>{titleize(event.source_type)}</td>
                  <td>{event.source_event_type ?? event.source_event_id}</td>
                  <td>{event.source_product_name ?? event.source_product_id ?? "—"}</td>
                  <td><StatusLabel value={event.processing_status} /></td>
                  <td>{event.signature_verified ? "Verified" : "Stored"}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {selected ? <JsonDrawer title={selected.source_event_id} value={selected.payload} onClose={() => setSelected(null)} /> : null}
        </Panel>
      ) : (
        <Panel title="Normalized events">
          <table className="data-table">
            <thead>
              <tr>
                <th>Time</th>
                <th>Type</th>
                <th>Product</th>
                <th>Transaction</th>
                <th>Amount</th>
                <th>Confidence</th>
              </tr>
            </thead>
            <tbody>
              {(normalized.data?.normalized_events ?? []).map((event) => (
                <tr key={event.id}>
                  <td>{formatDateTime(event.occurred_at)}</td>
                  <td>{titleize(event.event_type)}</td>
                  <td>{event.logical_product_name ?? event.source_product_name ?? "Unmapped"}</td>
                  <td className="mono-cell">{event.transaction_key ?? "—"}</td>
                  <td>{formatMoney(event.amount_minor, event.currency ?? "USD")}</td>
                  <td>{Math.round(event.confidence * 100)}%</td>
                </tr>
              ))}
            </tbody>
          </table>
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
        <table className="data-table">
          <thead>
            <tr>
              <th>Created</th>
              <th>Type</th>
              <th>Status</th>
              <th>Attempts</th>
              <th>Error</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {(jobs.data?.jobs ?? []).map((job) => (
              <tr key={job.id}>
                <td>{formatDateTime(job.created_at)}</td>
                <td>{job.job_type}</td>
                <td><StatusLabel value={job.status} /></td>
                <td>{job.attempts}/{job.max_attempts}</td>
                <td>{job.last_error ?? "—"}</td>
                <td>
                  {["failed", "dead"].includes(job.status) ? (
                    <button className="icon-button" onClick={() => retry.mutate(job.id)} title="Retry job">
                      <RefreshCw size={16} />
                    </button>
                  ) : null}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
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
          <table className="data-table compact-table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Apple bundle</th>
                <th>Google package</th>
                <th>Currency</th>
              </tr>
            </thead>
            <tbody>
              {(apps.data?.apps ?? []).map((app) => (
                <tr key={app.id}>
                  <td>{app.name}</td>
                  <td className="mono-cell">{app.apple_bundle_id ?? "—"}</td>
                  <td className="mono-cell">{app.google_package_name ?? "—"}</td>
                  <td>{app.default_currency ?? "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Panel>
        <Panel title="Local data">
          <div className="settings-actions">
            <a className="secondary-button" href={`${API_BASE_URL}/api/export/transactions.csv`}>
              <Database size={16} />
              Export transactions CSV
            </a>
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
        <select
          value={sourceType}
          onChange={(event) => {
            const next = event.target.value;
            setSourceType(next);
            setName(defaultSourceName(next));
            setCatchUpText("");
            setParseError(null);
          }}
        >
          <option value="revenuecat">RevenueCat</option>
          <option value="custom_api">Custom API</option>
          <option value="app_store">App Store</option>
          <option value="google_play">Google Play</option>
          <option value="stripe">Stripe</option>
          <option value="paddle">Paddle</option>
        </select>
      </Field>
      <Field label="Name">
        <input value={name} onChange={(event) => setName(event.target.value)} required />
      </Field>
      <Field label="App">
        <select value={appId} onChange={(event) => setAppId(event.target.value)}>
          <option value="">No app</option>
          {apps.map((app) => <option key={app.id} value={app.id}>{app.name}</option>)}
        </select>
      </Field>
      <Field label="Webhook secret">
        <input value={secret} onChange={(event) => setSecret(event.target.value)} placeholder="Optional shared secret" />
      </Field>
      {supportsCatchUp(sourceType) ? (
        <Field label="Catch-up credentials JSON (optional)">
          <textarea
            value={catchUpText}
            onChange={(event) => setCatchUpText(event.target.value)}
            placeholder={catchUpTemplate(sourceType)}
            rows={7}
          />
        </Field>
      ) : null}
      {parseError ? <p className="form-error">{parseError}</p> : null}
      {error ? <ErrorBlock error={error} /> : null}
      <button className="primary-button" disabled={pending || !name}>
        <Plus size={16} />
        Add source
      </button>
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
    <article className="source-card">
      <div className="source-card-head">
        <div>
          <strong>{source.name}</strong>
          <span>{titleize(source.source_type)} · {source.app_name ?? "No app selected"}</span>
        </div>
        <StatusLabel value={source.status} />
      </div>
      <div className="webhook-row">
        <code>{source.webhook_url}</code>
        <button
          className="icon-button"
          title="Copy webhook URL"
          onClick={async () => {
            await navigator.clipboard.writeText(source.webhook_url);
            setCopied(true);
            window.setTimeout(() => setCopied(false), 1200);
          }}
        >
          {copied ? <Check size={16} /> : <Copy size={16} />}
        </button>
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
        <button className="secondary-button" onClick={onTest}>
          <RefreshCw size={16} />
          Test
        </button>
        {onConfigureCatchUp ? (
          <button
            className="secondary-button"
            onClick={() => {
              setEditingCredentials((current) => !current);
              setCredentialsParseError(null);
            }}
          >
            <KeyRound size={16} />
            Configure
          </button>
        ) : null}
        {onCatchUp ? (
          <button className="secondary-button" onClick={onCatchUp}>
            <CloudDownload size={16} />
            Catch up
          </button>
        ) : null}
      </div>
      {editingCredentials && onConfigureCatchUp ? (
        <form className="source-credentials-form" onSubmit={saveCredentials}>
          <Field label="Catch-up credentials JSON">
            <textarea
              value={credentialsText}
              onChange={(event) => setCredentialsText(event.target.value)}
              placeholder={catchUpTemplate(source.source_type)}
              rows={7}
            />
          </Field>
          {credentialsParseError ? <p className="form-error">{credentialsParseError}</p> : null}
          {configureError ? <ErrorBlock error={configureError} /> : null}
          <div className="card-actions">
            <button className="secondary-button" type="button" onClick={() => setEditingCredentials(false)}>
              Cancel
            </button>
            <button className="primary-button" disabled={configurePending}>
              <Check size={16} />
              Save
            </button>
          </div>
        </form>
      ) : null}
      {source.last_error ? <p className="form-error">{source.last_error}</p> : null}
    </article>
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
      <input value={name} onChange={(event) => setName(event.target.value)} placeholder="App name" required />
      <input value={appleBundleId} onChange={(event) => setAppleBundleId(event.target.value)} placeholder="Apple bundle id" />
      <input value={googlePackageName} onChange={(event) => setGooglePackageName(event.target.value)} placeholder="Google package" />
      <input value={defaultCurrency} onChange={(event) => setDefaultCurrency(event.target.value.toUpperCase())} placeholder="USD" className="short-input" />
      <button className="primary-button" disabled={pending}>
        <Plus size={16} />
        Add
      </button>
      {error ? <ErrorBlock error={error} /> : null}
    </form>
  );
}

function TransactionTable({ transactions, compact = false }: { transactions: TransactionRecord[]; compact?: boolean }) {
  if (!transactions.length) return <EmptyState icon={<Receipt size={18} />} title="No transactions yet" />;
  return (
    <table className={compact ? "data-table compact-table" : "data-table"}>
      <thead>
        <tr>
          <th>Time</th>
          <th>Product</th>
          <th>Source</th>
          {!compact ? <th>Transaction</th> : null}
          <th>Amount</th>
          <th>Status</th>
          {!compact ? <th>Country</th> : null}
        </tr>
      </thead>
      <tbody>
        {transactions.map((transaction) => (
          <tr key={transaction.id}>
            <td>{formatDateTime(transaction.purchase_time)}</td>
            <td>{transaction.logical_product_name ?? transaction.source_product_name ?? "Unmapped"}</td>
            <td>{titleize(transaction.source_type)} · {titleize(transaction.platform)}</td>
            {!compact ? <td className="mono-cell">{transaction.transaction_key}</td> : null}
            <td>{formatMoney(transaction.amount_minor, transaction.currency)}</td>
            <td><StatusLabel value={transaction.status} /></td>
            {!compact ? <td>{transaction.country ?? "—"}</td> : null}
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function ProductTable({ products }: { products: LogicalProductRecord[] }) {
  if (!products.length) return <EmptyState icon={<Boxes size={18} />} title="No confirmed products" />;
  return (
    <table className="data-table">
      <thead>
        <tr>
          <th>Product</th>
          <th>Kind</th>
          <th>Period</th>
          <th>Category</th>
          <th>Sources</th>
        </tr>
      </thead>
      <tbody>
        {products.map((product) => (
          <tr key={product.id}>
            <td>{product.display_name}</td>
            <td>{titleize(product.product_kind)}</td>
            <td>{titleize(product.billing_period)}</td>
            <td>{product.reporting_category ?? "—"}</td>
            <td>{product.source_products.map((source) => source.external_product_id ?? source.display_name ?? source.id).join(", ") || "—"}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function Page({ title, actions, children }: { title: string; actions?: ReactNode; children: ReactNode }) {
  return (
    <div className="page">
      <header className="page-header">
        <h1>{title}</h1>
        {actions ? <div className="page-actions">{actions}</div> : null}
      </header>
      <div className="page-content">{children}</div>
    </div>
  );
}

function Panel({ title, actions, children }: { title: string; actions?: ReactNode; children: ReactNode }) {
  return (
    <section className="panel">
      <div className="panel-head">
        <h2>{title}</h2>
        {actions ? <div className="panel-actions">{actions}</div> : null}
      </div>
      {children}
    </section>
  );
}

function MetricCard({ label, value, state }: { label: string; value: string; state?: string }) {
  return (
    <div className="metric-card">
      <span>{label}</span>
      <strong>{value}</strong>
      <StatusLabel value={state ?? "live"} />
    </div>
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
  return (
    <div className={compact ? "filter-bar compact-filter" : "filter-bar"}>
      <input type="date" value={filters.from} onChange={(event) => onChange({ ...filters, from: event.target.value })} />
      <input type="date" value={filters.to} onChange={(event) => onChange({ ...filters, to: event.target.value })} />
      <select value={filters.app_id} onChange={(event) => onChange({ ...filters, app_id: event.target.value })}>
        <option value="all">All apps</option>
        {apps.data?.apps.map((app) => <option key={app.id} value={app.id}>{app.name}</option>)}
      </select>
      <select value={filters.platform} onChange={(event) => onChange({ ...filters, platform: event.target.value })}>
        <option value="all">All platforms</option>
        <option value="ios">iOS</option>
        <option value="android">Android</option>
        <option value="web">Web</option>
      </select>
      <input className="short-input" value={filters.currency} onChange={(event) => onChange({ ...filters, currency: event.target.value.toUpperCase() })} />
    </div>
  );
}

function useDashboardFilters() {
  const range = useMemo(last30Days, []);
  return useState<Record<string, string>>({
    ...range,
    app_id: "all",
    platform: "all",
    currency: "USD",
  });
}

function Segmented({ value, options, onChange }: { value: string; options: string[]; onChange: (value: string) => void }) {
  return (
    <div className="segmented">
      {options.map((option) => (
        <button key={option} className={option === value ? "active" : ""} onClick={() => onChange(option)} type="button">
          {titleize(option)}
        </button>
      ))}
    </div>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="field">
      <span>{label}</span>
      {children}
    </label>
  );
}

function StatusLabel({ value }: { value: string }) {
  const normalized = value.toLowerCase();
  return <span className={`status-label ${statusClass(normalized)}`}>{titleize(value)}</span>;
}

function statusClass(value: string) {
  if (["active", "paid", "renewed", "completed", "processed", "live", "verified"].includes(value)) return "good";
  if (["failed", "dead", "error", "refunded", "revoked"].includes(value)) return "bad";
  if (["estimated", "warning", "unmapped", "waiting_for_events", "queued", "running", "stored"].includes(value)) return "warn";
  return "neutral";
}

function ErrorBlock({ error }: { error: unknown }) {
  const message = error instanceof ApiError ? error.message : error instanceof Error ? error.message : "Request failed";
  return <div className="error-block">{message}</div>;
}

function EmptyState({ icon, title, action }: { icon: ReactNode; title: string; action?: ReactNode }) {
  return (
    <div className="empty-state">
      {icon}
      <span>{title}</span>
      {action}
    </div>
  );
}

function ChartFrame({ children }: { children: ReactNode }) {
  return <div className="chart-frame">{children}</div>;
}

function JsonDrawer({ title, value, onClose }: { title: string; value: unknown; onClose: () => void }) {
  return (
    <div className="drawer-backdrop" onClick={onClose}>
      <aside className="json-drawer" onClick={(event) => event.stopPropagation()}>
        <div className="drawer-head">
          <strong>{title}</strong>
          <button className="icon-button" onClick={onClose}>
            <X size={16} />
          </button>
        </div>
        <pre>{JSON.stringify(value, null, 2)}</pre>
      </aside>
    </div>
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
        environment: "production"
      },
      null,
      2,
    );
  }
  if (sourceType === "google_play") {
    return JSON.stringify(
      {
        pubsub_subscription: "projects/PROJECT_ID/subscriptions/SUBSCRIPTION_ID",
        service_account_json: {
          client_email: "revtern-pubsub@example.iam.gserviceaccount.com",
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
