// Shared synthetic native reports for the P2B frontend tests. Made-up values only; nothing here is a real observation.
import { REPORT_SCHEMA_VERSION, TARGET_REPOSITORY, TARGET_REPOSITORY_ID, TARGET_WORKFLOW } from "../ciAssessment";
import type { CiAssessmentReport } from "../types";

const SHA = "dd37b04f730569c3a8ad1b929ee6b80052ef22e2";

export const goodReport = (over: Partial<CiAssessmentReport> = {}): CiAssessmentReport => ({
  schemaVersion: REPORT_SCHEMA_VERSION,
  provenance: {
    eceRevision: "9f4e53e0a944a7681fb49957f0d5e30597c3148a", policyId: "atm-ci-two-runner-v1", policyFileSha256: "f".repeat(64),
    repository: TARGET_REPOSITORY, repositoryId: TARGET_REPOSITORY_ID, workflowPath: TARGET_WORKFLOW, commitSha: SHA, runId: 41217731446, runAttempt: 1,
  },
  outcome: "ASSESSED_LIMITATIONS", ci: "VERIFIED_PASS", availability: "NOT_PROVIDED", review: "UNREVIEWED", authorization: "AUTHORIZATION_PENDING",
  freshness: "HISTORICAL_BASELINE",
  limitations: ["NO_INDEPENDENT_REVIEW", "NO_HUMAN_AUTHORIZATION", "NOT_CURRENT_HEAD_ATTESTATION", "STANDALONE_RUN_ASSESSMENT", "COMMIT_SIGNATURE_NOT_VERIFIED", "HOSTED_RUNNERS_ONLY"],
  blocking: [], error: null, providerFailure: null,
  ...over,
});

