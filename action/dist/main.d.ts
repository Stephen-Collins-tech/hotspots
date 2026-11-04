interface FaultlineInputs {
    path: string;
    policy: string;
    minLrs?: string;
    config?: string;
    failOn: string;
    version: string;
    binaryPath?: string;
    githubToken: string;
    postComment: boolean;
}
interface FaultlineResult {
    violations: any[];
    passed: boolean;
    summary: string;
    reportPath?: string;
}
export declare function getInputs(): Promise<FaultlineInputs>;
export declare function policyGateEnabled(policy: string): boolean;
export declare function getPlatformInfo(): {
    platform: string;
    arch: string;
    ext: string;
    binaryName: string;
};
export declare function installFaultline(version: string, token?: string, binaryPath?: string): Promise<string>;
export declare function runDeltaAnalysis(binaryPath: string, inputs: FaultlineInputs): Promise<FaultlineResult>;
export declare function unwrapDelta(raw: any): any;
export declare function generateDiffSummary(result: any, baseSha: string, headSha: string): string;
export declare function commentMarker(analyzedPath: string): string;
export {};
//# sourceMappingURL=main.d.ts.map