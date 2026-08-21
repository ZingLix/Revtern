import type {
  AppRecord,
  AppMembersResponse,
  AppInvitationRecord,
  AuthIdentitiesResponse,
  AuthProvidersResponse,
  DataSourceRecord,
  DailyRevenuePoint,
  DailySubscriptionPoint,
  Id,
  JobRecord,
  InvitationPreview,
  LogicalProductRecord,
  MeResponse,
  NormalizedEventRecord,
  OverviewResponse,
  RawEventRecord,
  SetupStatus,
  SourceProductRecord,
  SubscriptionRecord,
  SubscriptionDetailResponse,
  SyncRunRecord,
  TransactionRecord,
  TransactionDetailResponse,
} from "@revtern/types";

export class ApiError extends Error {
  code: string;
  status: number;
  requestId?: string;

  constructor(message: string, status: number, code = "request_failed", requestId?: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
    this.requestId = requestId;
  }
}

export interface CatalogConfirmation {
  app_id: Id;
  logical_products: Array<{
    client_id: string;
    existing_logical_product_id?: Id | null;
    display_name: string;
    product_kind: string;
    billing_period: string;
    reporting_category?: string | null;
  }>;
  mappings: Array<{
    source_product_id: Id;
    logical_product_client_id: string;
    mapping_method?: string;
  }>;
  ignored_source_product_ids: Id[];
}

export interface RevternApiOptions {
  baseUrl?: string;
  accessToken?: () => string | null | undefined | Promise<string | null | undefined>;
}

export class RevternApi {
  private readonly baseUrl: string;
  private readonly accessToken?: RevternApiOptions["accessToken"];

  constructor(options: string | RevternApiOptions = "") {
    if (typeof options === "string") {
      this.baseUrl = options;
      return;
    }
    this.baseUrl = options.baseUrl ?? "";
    this.accessToken = options.accessToken;
  }

  setupStatus() {
    return this.get<SetupStatus>("/api/setup/status");
  }

  setupOwner(input: { email: string; password: string; workspace_name: string }) {
    return this.post<{ created: boolean }>("/api/setup/owner", input);
  }

  authProviders() {
    return this.get<AuthProvidersResponse>("/api/auth/providers");
  }

  authIdentities() {
    return this.get<AuthIdentitiesResponse>("/api/auth/identities");
  }

  register(input: { email: string; password: string; display_name?: string; invite_token?: string }) {
    return this.post<{ created: boolean; user_id: Id; workspace_id: Id; accepted_app_id?: Id | null }>("/api/registration", input);
  }

  invitation(token: string) {
    return this.get<{ invitation: InvitationPreview }>(`/api/invitations/${encodeURIComponent(token)}`);
  }

  acceptInvitation(token: string) {
    return this.post<{ accepted: boolean; app_id: Id }>(`/api/invitations/${encodeURIComponent(token)}`, {});
  }

  login(input: { email: string; password: string }) {
    return this.post<{ logged_in: boolean }>("/api/session", input);
  }

  mobileLogin(input: { email: string; password: string }) {
    return this.post<{ logged_in: boolean; access_token: string; token_type: "Bearer"; expires_in: number }>("/api/mobile/session", input);
  }

  logout() {
    return this.delete<{ logged_out: boolean }>("/api/session");
  }

  mobileLogout() {
    return this.delete<{ logged_out: boolean }>("/api/mobile/session");
  }

  me() {
    return this.get<MeResponse>("/api/me");
  }

  apps() {
    return this.get<{ apps: AppRecord[] }>("/api/apps");
  }

  createApp(input: Partial<AppRecord> & { name: string }) {
    return this.post<{ app: AppRecord }>("/api/apps", input);
  }

  updateApp(id: Id, input: Partial<AppRecord> & { name: string }) {
    return this.patch<{ app: AppRecord }>(`/api/apps/${id}`, input);
  }

  appMembers(id: Id) {
    return this.get<AppMembersResponse>(`/api/apps/${id}/members`);
  }

  inviteAppMember(id: Id, input: { email: string; role: string }) {
    return this.post<{ invitation: AppInvitationRecord }>(`/api/apps/${id}/invitations`, input);
  }

  revokeAppInvitation(appId: Id, invitationId: Id) {
    return this.delete<{ revoked: boolean }>(`/api/apps/${appId}/invitations/${invitationId}`);
  }

  updateAppMember(appId: Id, userId: Id, input: { role: string }) {
    return this.patch<{ updated: boolean }>(`/api/apps/${appId}/members/${userId}`, input);
  }

  removeAppMember(appId: Id, userId: Id) {
    return this.delete<{ removed: boolean }>(`/api/apps/${appId}/members/${userId}`);
  }

  dataSources(params: Record<string, string | undefined> = {}) {
    return this.get<{ data_sources: DataSourceRecord[] }>(`/api/data-sources${query(params)}`);
  }

  createDataSource(input: { source_type: string; name: string; app_id?: Id | null; credentials?: Record<string, unknown> }) {
    return this.post<{ data_source: DataSourceRecord }>("/api/data-sources", input);
  }

  updateDataSourceCredentials(id: Id, input: { credentials?: Record<string, unknown> }) {
    return this.patch<{ data_source: DataSourceRecord }>(`/api/data-sources/${id}/credentials`, input);
  }

  testDataSource(id: Id) {
    return this.post<{ sync_run: SyncRunRecord }>(`/api/data-sources/${id}/test`, {});
  }

  catchUpDataSource(id: Id, input: { from?: string; to?: string; limit?: number; cursor?: string } = {}) {
    return this.post<{ sync_run: SyncRunRecord }>(`/api/data-sources/${id}/catch-up`, input);
  }

  logicalProducts(params: Record<string, string | undefined> = {}) {
    return this.get<{ logical_products: LogicalProductRecord[] }>(`/api/products/logical${query(params)}`);
  }

  sourceProducts(params: Record<string, string | undefined> = {}) {
    return this.get<{ source_products: SourceProductRecord[] }>(`/api/products/source${query(params)}`);
  }

  confirmCatalog(input: CatalogConfirmation) {
    return this.post<{ confirmed: boolean }>("/api/products/catalog-confirmations", input);
  }

  rawEvents(params: Record<string, string | undefined> = {}) {
    return this.get<{ raw_events: RawEventRecord[] }>(`/api/events/raw${query(params)}`);
  }

  normalizedEvents(params: Record<string, string | undefined> = {}) {
    return this.get<{ normalized_events: NormalizedEventRecord[] }>(`/api/events/normalized${query(params)}`);
  }

  transactions(params: Record<string, string | undefined> = {}) {
    return this.get<{ transactions: TransactionRecord[] }>(`/api/transactions${query(params)}`);
  }

  transaction(id: Id) {
    return this.get<TransactionDetailResponse>(`/api/transactions/${id}`);
  }

  subscriptions(params: Record<string, string | undefined> = {}) {
    return this.get<{ subscriptions: SubscriptionRecord[] }>(`/api/subscriptions${query(params)}`);
  }

  subscription(id: Id) {
    return this.get<SubscriptionDetailResponse>(`/api/subscriptions/${id}`);
  }

  overview(params: Record<string, string | undefined> = {}) {
    return this.get<OverviewResponse>(`/api/metrics/overview${query(params)}`);
  }

  revenueTimeseries(params: Record<string, string | undefined> = {}) {
    return this.get<{ series: DailyRevenuePoint[] }>(`/api/metrics/revenue-timeseries${query(params)}`);
  }

  subscriptionTimeseries(params: Record<string, string | undefined> = {}) {
    return this.get<{ series: DailySubscriptionPoint[] }>(`/api/metrics/subscription-timeseries${query(params)}`);
  }

  breakdown(params: Record<string, string | undefined> = {}) {
    return this.get<{ by: string; items: Array<{ label: string; gross_revenue_minor: number; refund_amount_minor: number; transaction_count: number }> }>(
      `/api/metrics/breakdown${query(params)}`,
    );
  }

  syncRuns(params: Record<string, string | undefined> = {}) {
    return this.get<{ sync_runs: SyncRunRecord[] }>(`/api/sync-runs${query(params)}`);
  }

  jobs(params: Record<string, string | undefined> = {}) {
    return this.get<{ jobs: JobRecord[] }>(`/api/jobs${query(params)}`);
  }

  retryJob(id: Id) {
    return this.post<{ job: JobRecord }>(`/api/jobs/${id}/retry`, {});
  }

  seedDemo() {
    return this.post<{ seeded: boolean; events_inserted: number; app_id: Id; data_source_id: Id }>("/api/demo/seed", {});
  }

  webhookUrl(sourceType: string, sourceId: Id) {
    return `${this.baseUrl}/webhooks/${sourceType.replaceAll("_", "-")}/${sourceId}`;
  }

  oidcStartUrl(input: { returnTo?: string; inviteToken?: string; link?: boolean } = {}) {
    const search = new URLSearchParams();
    if (input.returnTo) search.set("return_to", input.returnTo);
    if (input.inviteToken) search.set("invite_token", input.inviteToken);
    const suffix = search.size ? `?${search.toString()}` : "";
    return `${this.baseUrl}/api/auth/oidc/${input.link ? "link" : "start"}${suffix}`;
  }

  private get<T>(path: string) {
    return this.request<T>(path, { method: "GET" });
  }

  private post<T>(path: string, body: unknown) {
    return this.request<T>(path, { method: "POST", body: JSON.stringify(body) });
  }

  private patch<T>(path: string, body: unknown) {
    return this.request<T>(path, { method: "PATCH", body: JSON.stringify(body) });
  }

  private delete<T>(path: string) {
    return this.request<T>(path, { method: "DELETE" });
  }

  private async request<T>(path: string, init: RequestInit): Promise<T> {
    const headers = new Headers(init.headers);
    if (init.body) {
      headers.set("content-type", "application/json");
    }
    const token = await this.accessToken?.();
    if (token) {
      headers.set("authorization", `Bearer ${token}`);
    }
    const csrf = readCookie("revtern_csrf");
    if (csrf && init.method && !["GET", "HEAD", "OPTIONS"].includes(init.method)) {
      headers.set("x-csrf-token", csrf);
    }
    const response = await fetch(`${this.baseUrl}${path}`, {
      ...init,
      headers,
      credentials: "include",
    });
    if (!response.ok) {
      let message = response.statusText;
      let code = "request_failed";
      let requestId: string | undefined;
      try {
        const body = (await response.json()) as { error?: { message?: string; code?: string; request_id?: string } };
        message = body.error?.message ?? message;
        code = body.error?.code ?? code;
        requestId = body.error?.request_id;
      } catch {
        // Keep the HTTP status text.
      }
      throw new ApiError(message, response.status, code, requestId);
    }
    if (response.status === 204) {
      return undefined as T;
    }
    return (await response.json()) as T;
  }
}

function query(params: Record<string, string | undefined>) {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value && value !== "all") {
      search.set(key, value);
    }
  }
  const text = search.toString();
  return text ? `?${text}` : "";
}

function readCookie(name: string) {
  if (typeof document === "undefined") {
    return undefined;
  }
  return document.cookie
    .split("; ")
    .find((part) => part.startsWith(`${name}=`))
    ?.split("=")
    .slice(1)
    .join("=");
}
