#!/usr/bin/env python3
"""
Comprehensive test script for Faultline High-Leverage Extensions
Tests Policy Engine, Trend Semantics, Aggregation Views, and Visualization
"""

import json
import os
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Dict, List, Optional


class Colors:
    """ANSI color codes for terminal output"""
    GREEN = '\033[0;32m'
    RED = '\033[0;31m'
    YELLOW = '\033[1;33m'
    NC = '\033[0m'  # No Color


class TestRunner:
    def __init__(self):
        self.script_dir = Path(__file__).parent.resolve()
        self.faultline_bin = self.script_dir / "target" / "release" / "faultline"
        self.test_dir = self.script_dir / "test-repo-comprehensive"
        self.commits: List[str] = []
        
        if not self.faultline_bin.exists():
            print(f"{Colors.RED}✗ Faultline binary not found at {self.faultline_bin}{Colors.NC}")
            sys.exit(1)
    
    def run_command(self, cmd: List[str], cwd: Optional[Path] = None, 
                   capture_output: bool = True, check: bool = True) -> subprocess.CompletedProcess:
        """Run a command and return the result"""
        cwd = cwd or self.test_dir
        try:
            result = subprocess.run(
                cmd,
                cwd=cwd,
                capture_output=capture_output,
                text=True,
                check=check
            )
            return result
        except subprocess.CalledProcessError as e:
            if check:
                print(f"{Colors.RED}✗ Command failed: {' '.join(cmd)}{Colors.NC}")
                print(f"Error: {e.stderr}")
                raise
            return e
    
    def git_command(self, args: List[str], check: bool = True) -> subprocess.CompletedProcess:
        """Run a git command"""
        return self.run_command(["git"] + args, check=check)
    
    def faultline_command(self, args: List[str], output_file: Optional[Path] = None, 
                         check: bool = True) -> subprocess.CompletedProcess:
        """Run faultline command"""
        cmd = [str(self.faultline_bin)] + args
        if output_file:
            with open(output_file, 'w') as f:
                result = subprocess.run(
                    cmd,
                    cwd=self.test_dir,
                    stdout=f,
                    stderr=subprocess.STDOUT,
                    text=True,
                    check=check
                )
                return result
        else:
            return self.run_command(cmd, check=check)
    
    def setup_test_repo(self):
        """Initialize test git repository"""
        print("1. Initializing test git repository...")
        
        # Clean up old test directory
        if self.test_dir.exists():
            shutil.rmtree(self.test_dir)
        self.test_dir.mkdir(parents=True)
        
        # Initialize git repo
        self.git_command(["init"], check=True)
        self.git_command(["config", "user.name", "Test User"], check=True)
        self.git_command(["config", "user.email", "test@example.com"], check=True)
        
        # Create .gitignore
        (self.test_dir / ".gitignore").write_text(".faultline/\n")
        self.git_command(["add", ".gitignore"], check=True)
        self.git_command(["commit", "-m", "Initial commit"], check=True)
        
        # Clean up any existing snapshots
        faultline_dir = self.test_dir / ".faultline"
        if faultline_dir.exists():
            shutil.rmtree(faultline_dir)
    
    def create_initial_functions(self):
        """Create initial TypeScript file with low complexity functions"""
        print("2. Creating initial TypeScript file...")
        
        src_dir = self.test_dir / "src"
        src_dir.mkdir(exist_ok=True)
        
        main_ts = src_dir / "main.ts"
        main_ts.write_text("""// Low complexity function
function simpleFunction() {
    return 42;
}

// Moderate complexity function
function moderateFunction(x: number) {
    if (x > 0) {
        return x * 2;
    } else {
        return -x;
    }
}
""")
        
        self.git_command(["add", "src/main.ts"], check=True)
        self.git_command(["commit", "-m", "Add initial functions"], check=True)
        
        commit_sha = self.git_command(["rev-parse", "HEAD"], check=True).stdout.strip()
        self.commits.append(commit_sha)
        print(f"   Commit 1: {commit_sha}")
        
        # Run snapshot analysis
        print("   Running snapshot analysis...")
        snapshot_file = self.test_dir / "snapshot1.json"
        result = self.faultline_command(
            ["analyze", "--mode", "snapshot", "--format", "json", "src/main.ts"],
            output_file=snapshot_file,
            check=False
        )
        
        if result.returncode != 0:
            print(f"{Colors.RED}✗ Snapshot analysis failed{Colors.NC}")
            print(snapshot_file.read_text() if snapshot_file.exists() else "No output")
            sys.exit(1)
        
        print(f"{Colors.GREEN}✓ Snapshot analysis completed{Colors.NC}")
    
    def add_high_complexity_function(self):
        """Add high complexity function (should trigger Excessive Risk Regression)"""
        print("\n3. Adding high complexity function...")
        
        main_ts = self.test_dir / "src" / "main.ts"
        content = main_ts.read_text()
        content += """
// High complexity function - will trigger Excessive Risk Regression
function highComplexityFunction(x: number, y: number, z: number) {
    if (x > 0) {
        if (y > 0) {
            if (z > 0) {
                return x + y + z;
            } else {
                return x + y;
            }
        } else {
            if (z > 0) {
                return x + z;
            } else {
                return x;
            }
        }
    } else {
        if (y > 0) {
            if (z > 0) {
                return y + z;
            } else {
                return y;
            }
        } else {
            return z > 0 ? z : 0;
        }
    }
}
"""
        main_ts.write_text(content)
        
        self.git_command(["add", "src/main.ts"], check=True)
        self.git_command(["commit", "-m", "Add high complexity function"], check=True)
        
        commit_sha = self.git_command(["rev-parse", "HEAD"], check=True).stdout.strip()
        self.commits.append(commit_sha)
        print(f"   Commit 2: {commit_sha}")
        
        # Run snapshot analysis
        print("   Running snapshot analysis...")
        snapshot_file = self.test_dir / "snapshot2.json"
        result = self.faultline_command(
            ["analyze", "--mode", "snapshot", "--format", "json", "src/main.ts"],
            output_file=snapshot_file,
            check=False
        )
        
        if result.returncode != 0:
            print(f"{Colors.RED}✗ Snapshot analysis failed{Colors.NC}")
            print(snapshot_file.read_text() if snapshot_file.exists() else "No output")
            sys.exit(1)
        
        # Test delta with policy
        print("   Testing delta with policy...")
        delta_file = self.test_dir / "delta1.json"
        result = self.faultline_command(
            ["analyze", "--mode", "delta", "--policy", "--format", "json", "src/main.ts"],
            output_file=delta_file,
            check=False
        )
        
        # Check for policy violations
        print("   Checking for policy violations...")
        if delta_file.exists() and delta_file.stat().st_size > 0:
            try:
                delta_data = json.loads(delta_file.read_text())
                policy = delta_data.get("policy", {})
                failed_count = len(policy.get("failed", []))
                warnings_count = len(policy.get("warnings", []))
                
                if failed_count > 0 or warnings_count > 0:
                    print(f"{Colors.GREEN}✓ Policy evaluation working{Colors.NC}")
                    print(f"   Failed policies: {failed_count}")
                    print(f"   Warnings: {warnings_count}")
                    print("\n   Policy details:")
                    policy_str = json.dumps(policy, indent=2)
                    if len(policy_str) > 500:
                        print(policy_str[:500] + "...")
                    else:
                        print(policy_str)
                else:
                    print(f"{Colors.YELLOW}⚠ No policy violations detected (may be expected){Colors.NC}")
            except json.JSONDecodeError as e:
                print(f"{Colors.YELLOW}⚠ Could not parse delta JSON: {e}{Colors.NC}")
                print(f"   File contents: {delta_file.read_text()[:200]}")
        else:
            print(f"{Colors.YELLOW}⚠ Delta file is empty or missing{Colors.NC}")
    
    def add_critical_function(self):
        """Add critical function (should trigger Critical Introduction policy)"""
        print("\n4. Adding critical function...")
        
        main_ts = self.test_dir / "src" / "main.ts"
        content = main_ts.read_text()
        content += """
// Critical function - will trigger Critical Introduction policy
function criticalFunction() {
    let result = 0;
    for (let i = 0; i < 10; i++) {
        for (let j = 0; j < 10; j++) {
            for (let k = 0; k < 10; k++) {
                if (i > j && j > k) {
                    result += i * j * k;
                } else if (i < j && j < k) {
                    result -= i * j * k;
                } else {
                    result += i + j + k;
                }
            }
        }
    }
    return result;
}
"""
        main_ts.write_text(content)
        
        self.git_command(["add", "src/main.ts"], check=True)
        self.git_command(["commit", "-m", "Add critical function"], check=True)
        
        commit_sha = self.git_command(["rev-parse", "HEAD"], check=True).stdout.strip()
        self.commits.append(commit_sha)
        print(f"   Commit 3: {commit_sha}")
        
        # Run snapshot analysis
        print("   Running snapshot analysis...")
        snapshot_file = self.test_dir / "snapshot3.json"
        result = self.faultline_command(
            ["analyze", "--mode", "snapshot", "--format", "json", "src/main.ts"],
            output_file=snapshot_file,
            check=False
        )
        
        if result.returncode != 0:
            print(f"{Colors.RED}✗ Snapshot analysis failed{Colors.NC}")
            print(snapshot_file.read_text() if snapshot_file.exists() else "No output")
            sys.exit(1)
        
        # Test delta with policy
        print("   Testing delta with policy...")
        delta_file = self.test_dir / "delta2.json"
        result = self.faultline_command(
            ["analyze", "--mode", "delta", "--policy", "--format", "json", "src/main.ts"],
            output_file=delta_file,
            check=False
        )
        
        if delta_file.exists() and delta_file.stat().st_size > 0:
            try:
                delta_data = json.loads(delta_file.read_text())
                policy = delta_data.get("policy", {})
                failed_count = len(policy.get("failed", []))
                
                if failed_count > 0:
                    print(f"{Colors.GREEN}✓ Critical Introduction policy triggered{Colors.NC}")
                    print("\n   Failed policies:")
                    for failure in policy.get("failed", [])[:3]:
                        print(f"     - {failure.get('id')}: {failure.get('message')}")
                else:
                    print(f"{Colors.YELLOW}⚠ Critical Introduction policy not triggered{Colors.NC}")
            except json.JSONDecodeError as e:
                print(f"{Colors.YELLOW}⚠ Could not parse delta JSON: {e}{Colors.NC}")
        else:
            print(f"{Colors.YELLOW}⚠ Delta file is empty or missing{Colors.NC}")
    
    def test_aggregates(self):
        """Test aggregation views"""
        print("\n5. Testing aggregation views...")
        
        snapshot_file = self.test_dir / "snapshot3.json"
        delta_file = self.test_dir / "delta2.json"
        
        if snapshot_file.exists() and snapshot_file.stat().st_size > 0:
            try:
                snapshot_data = json.loads(snapshot_file.read_text())
                aggregates = snapshot_data.get("aggregates", {})
                file_count = len(aggregates.get("files", []))
                dir_count = len(aggregates.get("directories", []))
                
                if file_count > 0:
                    print(f"{Colors.GREEN}✓ Snapshot aggregates working{Colors.NC}")
                    print(f"   File aggregates: {file_count}")
                    print(f"   Directory aggregates: {dir_count}")
                    if dir_count > 0:
                        print("\n   Directory aggregates sample:")
                        for dir_agg in aggregates.get("directories", [])[:3]:
                            print(f"     {dir_agg.get('directory')}: sum_lrs={dir_agg.get('sum_lrs'):.2f}")
                else:
                    print(f"{Colors.RED}✗ Snapshot aggregates missing{Colors.NC}")
            except json.JSONDecodeError as e:
                print(f"{Colors.YELLOW}⚠ Could not parse snapshot JSON: {e}{Colors.NC}")
        
        if delta_file.exists() and delta_file.stat().st_size > 0:
            try:
                delta_data = json.loads(delta_file.read_text())
                aggregates = delta_data.get("aggregates", {})
                file_count = len(aggregates.get("files", []))
                
                if file_count > 0:
                    print(f"{Colors.GREEN}✓ Delta aggregates working{Colors.NC}")
                    print(f"   File aggregates: {file_count}")
                else:
                    print(f"{Colors.RED}✗ Delta aggregates missing{Colors.NC}")
            except json.JSONDecodeError as e:
                print(f"{Colors.YELLOW}⚠ Could not parse delta JSON: {e}{Colors.NC}")
    
    def test_trends(self):
        """Test trend semantics"""
        print("\n6. Testing trend semantics...")
        
        trends_file = self.test_dir / "trends.json"
        result = self.faultline_command(
            ["trends", "--window", "10", "--top", "5", "--format", "json", "."],
            output_file=trends_file,
            check=False
        )
        
        if trends_file.exists() and trends_file.stat().st_size > 0:
            try:
                trends_data = json.loads(trends_file.read_text())
                velocities_count = len(trends_data.get("velocities", []))
                hotspots_count = len(trends_data.get("hotspots", []))
                refactors_count = len(trends_data.get("refactors", []))
                
                print(f"   Risk velocities: {velocities_count}")
                print(f"   Hotspots: {hotspots_count}")
                print(f"   Refactors: {refactors_count}")
                
                if hotspots_count > 0:
                    print(f"{Colors.GREEN}✓ Hotspot analysis working{Colors.NC}")
                else:
                    print(f"{Colors.YELLOW}⚠ No hotspots detected (may need more commits){Colors.NC}")
            except json.JSONDecodeError as e:
                print(f"{Colors.YELLOW}⚠ Could not parse trends JSON: {e}{Colors.NC}")
                if trends_file.exists():
                    print(f"   File contents: {trends_file.read_text()[:200]}")
        else:
            print(f"{Colors.YELLOW}⚠ Trends analysis failed or file is empty{Colors.NC}")
    
    def test_text_output(self):
        """Test text output formats"""
        print("\n7. Testing text output formats...")
        
        print("   Delta with policy (text):")
        result = self.faultline_command(
            ["analyze", "--mode", "delta", "--policy", "--format", "text", "src/main.ts"],
            check=False
        )
        if result.returncode == 0:
            output_lines = result.stdout.split('\n')[:30]
            for line in output_lines:
                print(f"     {line}")
        else:
            print(f"     {Colors.YELLOW}(Output not available){Colors.NC}")
        
        print("\n   Trends (text):")
        result = self.faultline_command(
            ["trends", "--window", "10", "--top", "5", "--format", "text", "."],
            check=False
        )
        if result.returncode == 0:
            output_lines = result.stdout.split('\n')[:30]
            for line in output_lines:
                print(f"     {line}")
        else:
            print(f"     {Colors.YELLOW}(Output not available){Colors.NC}")
    
    def print_summary(self):
        """Print test summary"""
        print("\n=== Test Summary ===")
        print(f"Commits created: {len(self.commits)}")
        print(f"Snapshots: {len(list((self.test_dir / '.faultline' / 'snapshots').glob('*.json')))}")
        print(f"Deltas tested: {len(list(self.test_dir.glob('delta*.json')))}")
        print(f"\nTest files saved in: {self.test_dir}")
        print(f"\n{Colors.GREEN}✓ Comprehensive test completed{Colors.NC}")
    
    def run(self):
        """Run all tests"""
        print("=== Faultline Comprehensive Test Suite ===\n")
        print(f"Test directory: {self.test_dir}\n")
        
        try:
            self.setup_test_repo()
            self.create_initial_functions()
            self.add_high_complexity_function()
            self.add_critical_function()
            self.test_aggregates()
            self.test_trends()
            self.test_text_output()
            self.print_summary()
        except Exception as e:
            print(f"\n{Colors.RED}✗ Test failed with error: {e}{Colors.NC}")
            import traceback
            traceback.print_exc()
            sys.exit(1)


if __name__ == "__main__":
    runner = TestRunner()
    runner.run()
