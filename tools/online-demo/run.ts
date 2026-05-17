const GITHUB_GRAPHQL_ENDPOINT = 'https://api.github.com/graphql';
const TARGET_OWNER = 'theolundqvist';
const TARGET_REPO = 'pr-cockpit-v2';
const TARGET_PR_NUMBER = 1;

type GraphqlResponse<T> = {
  data?: T;
  errors?: Array<{ message: string }>;
};

type PrDetailQuery = {
  repository: {
    pullRequest: {
      id: string;
      number: number;
      title: string;
      state: string;
    } | null;
  } | null;
};

type InboxRefreshQuery = {
  nodes: Array<
    | {
        __typename: 'PullRequest';
        id: string;
        number: number;
        title: string;
        state: string;
      }
    | { __typename: string; id?: string }
    | null
  >;
};

async function graphql<TData>(
  token: string,
  query: string,
  variables: Record<string, unknown>
): Promise<TData> {
  const response = await fetch(GITHUB_GRAPHQL_ENDPOINT, {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${token}`,
      'Content-Type': 'application/json',
      Accept: 'application/vnd.github+json',
      'User-Agent': 'pr-cockpit-online-demo'
    },
    body: JSON.stringify({ query, variables })
  });

  if (!response.ok) {
    const body = await response.text();
    throw new Error(`GitHub GraphQL request failed (${response.status}): ${body.slice(0, 400)}`);
  }

  const payload = (await response.json()) as GraphqlResponse<TData>;
  if (payload.errors && payload.errors.length > 0) {
    throw new Error(`GitHub GraphQL errors: ${payload.errors.map((entry) => entry.message).join(' | ')}`);
  }
  if (!payload.data) {
    throw new Error('GitHub GraphQL returned no data.');
  }

  return payload.data;
}

async function run(): Promise<void> {
  const token = process.env.GITHUB_TOKEN?.trim();
  if (!token) {
    console.log('[online-demo] skip: GITHUB_TOKEN is not set; live API demo is optional and non-gating.');
    return;
  }

  const prDetail = await graphql<PrDetailQuery>(
    token,
    `
      query PrDetail($owner: String!, $repo: String!, $number: Int!) {
        repository(owner: $owner, name: $repo) {
          pullRequest(number: $number) {
            id
            number
            title
            state
          }
        }
      }
    `,
    { owner: TARGET_OWNER, repo: TARGET_REPO, number: TARGET_PR_NUMBER }
  );

  const pr = prDetail.repository?.pullRequest;
  if (!pr) {
    throw new Error(
      `[online-demo] repository ${TARGET_OWNER}/${TARGET_REPO} does not expose PR #${TARGET_PR_NUMBER}`
    );
  }

  const inboxRefresh = await graphql<InboxRefreshQuery>(
    token,
    `
      query InboxRefresh($ids: [ID!]!) {
        nodes(ids: $ids) {
          __typename
          ... on PullRequest {
            id
            number
            title
            state
          }
        }
      }
    `,
    { ids: [pr.id] }
  );

  const matchedPullRequest = inboxRefresh.nodes.find(
    (node): node is Extract<NonNullable<InboxRefreshQuery['nodes'][number]>, { __typename: 'PullRequest' }> =>
      !!node && node.__typename === 'PullRequest' && node.id === pr.id
  );

  if (!matchedPullRequest) {
    throw new Error(`[online-demo] InboxRefresh did not return the target PR node ${pr.id}`);
  }

  console.log(
    `[online-demo] success: PR detail + inbox refresh resolved for ${TARGET_OWNER}/${TARGET_REPO}#${matchedPullRequest.number} (${matchedPullRequest.state})`
  );
}

run().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
