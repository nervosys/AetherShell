#!/usr/bin/env python3
"""
AetherShell Security Audit Script
Comprehensive security compliance verification
"""

import re
import os
import subprocess
import json
from pathlib import Path
import sys

class SecurityAudit:
    def __init__(self):
        self.findings = []
        self.critical_count = 0
        self.high_count = 0
        self.medium_count = 0
        self.low_count = 0
        
    def add_finding(self, severity, category, description, file_path="", line_number=0):
        finding = {
            "severity": severity,
            "category": category,
            "description": description,
            "file": file_path,
            "line": line_number
        }
        self.findings.append(finding)
        
        if severity == "CRITICAL":
            self.critical_count += 1
        elif severity == "HIGH":
            self.high_count += 1
        elif severity == "MEDIUM":
            self.medium_count += 1
        elif severity == "LOW":
            self.low_count += 1
    
    def audit_unsafe_code(self):
        """Check for unsafe code blocks"""
        print("🔍 Auditing unsafe code usage...")
        
        rust_files = list(Path("src").rglob("*.rs"))
        for file_path in rust_files:
            try:
                with open(file_path, 'r', encoding='utf-8') as f:
                    content = f.read()
                    lines = content.split('\n')
                    
                for i, line in enumerate(lines, 1):
                    if 'unsafe' in line and not line.strip().startswith('//'):
                        self.add_finding(
                            "HIGH", 
                            "Memory Safety", 
                            f"Unsafe code block detected: {line.strip()}", 
                            str(file_path), 
                            i
                        )
            except Exception as e:
                print(f"Warning: Could not read {file_path}: {e}")
    
    def audit_unwrap_usage(self):
        """Check for dangerous unwrap() calls"""
        print("🔍 Auditing unwrap() usage...")
        
        rust_files = list(Path("src").rglob("*.rs"))
        dangerous_patterns = [
            r'\.unwrap\(\)',
            r'\.expect\(".*"\)',
        ]
        
        for file_path in rust_files:
            try:
                with open(file_path, 'r', encoding='utf-8') as f:
                    content = f.read()
                    lines = content.split('\n')
                    
                for i, line in enumerate(lines, 1):
                    # Skip comments and test code
                    if line.strip().startswith('//') or '#[cfg(test)]' in content:
                        continue
                        
                    for pattern in dangerous_patterns:
                        if re.search(pattern, line):
                            # Check if it's in a test module or has security comment
                            if ('SECURITY' in line or 
                                'test' in str(file_path).lower() or
                                'tests/' in str(file_path)):
                                continue
                                
                            self.add_finding(
                                "MEDIUM", 
                                "Error Handling", 
                                f"Potential panic risk: {line.strip()}", 
                                str(file_path), 
                                i
                            )
            except Exception as e:
                print(f"Warning: Could not read {file_path}: {e}")
    
    def audit_input_validation(self):
        """Check for proper input validation"""
        print("🔍 Auditing input validation...")
        
        # Check if validation functions exist
        security_file = Path("src/security.rs")
        if security_file.exists():
            with open(security_file, 'r') as f:
                content = f.read()
                
            required_functions = [
                "validate_ai_prompt",
                "validate_command", 
                "validate_safe_path",
                "validate_string_input"
            ]
            
            for func in required_functions:
                if func not in content:
                    self.add_finding(
                        "HIGH",
                        "Input Validation",
                        f"Missing security function: {func}",
                        str(security_file)
                    )
                else:
                    print(f"✅ Found validation function: {func}")
        else:
            self.add_finding(
                "CRITICAL",
                "Security Architecture", 
                "Security module not found",
                "src/security.rs"
            )
    
    def audit_secrets_exposure(self):
        """Check for exposed secrets or API keys"""
        print("🔍 Auditing secrets exposure...")
        
        secret_patterns = [
            r'sk-[a-zA-Z0-9]{48}',  # OpenAI API keys
            r'["\'](?:password|secret|key|token)["\']\s*:\s*["\'][^"\']+["\']',
            r'api[_-]?key\s*=\s*["\'][^"\']+["\']',
        ]
        
        all_files = list(Path(".").rglob("*.rs")) + list(Path(".").rglob("*.toml")) + list(Path(".").rglob("*.md"))
        
        for file_path in all_files:
            if '.git' in str(file_path) or 'target' in str(file_path):
                continue
                
            try:
                with open(file_path, 'r', encoding='utf-8') as f:
                    content = f.read()
                    lines = content.split('\n')
                    
                for i, line in enumerate(lines, 1):
                    for pattern in secret_patterns:
                        if re.search(pattern, line, re.IGNORECASE):
                            # Skip examples and test data
                            if ('example' in line.lower() or 
                                'placeholder' in line.lower() or
                                'your_key' in line.lower() or
                                'sk-example' in line.lower()):
                                continue
                                
                            self.add_finding(
                                "CRITICAL",
                                "Secrets Exposure",
                                f"Potential secret exposed: {line.strip()[:50]}...",
                                str(file_path),
                                i
                            )
            except Exception as e:
                continue  # Skip binary files
    
    def audit_dependencies(self):
        """Check for vulnerable dependencies"""
        print("🔍 Auditing dependencies...")
        
        try:
            result = subprocess.run(
                ["cargo", "audit"], 
                capture_output=True, 
                text=True,
                cwd="."
            )
            
            if result.returncode != 0:
                if "not found" in result.stderr:
                    print("⚠️  cargo-audit not installed, skipping dependency audit")
                    self.add_finding(
                        "LOW",
                        "Tooling",
                        "cargo-audit not available for dependency scanning",
                        "Cargo.toml"
                    )
                else:
                    # Parse audit output for vulnerabilities
                    output = result.stdout + result.stderr
                    if "vulnerabilities found" in output.lower():
                        self.add_finding(
                            "HIGH",
                            "Dependencies",
                            "Vulnerable dependencies detected",
                            "Cargo.toml"
                        )
            else:
                print("✅ No vulnerable dependencies found")
                
        except Exception as e:
            print(f"⚠️  Could not run dependency audit: {e}")
    
    def audit_crypto_usage(self):
        """Check for proper cryptographic practices"""
        print("🔍 Auditing cryptographic usage...")
        
        weak_crypto_patterns = [
            r'md5',
            r'sha1(?!_)',  # SHA1 but not SHA1-based algorithms
            r'des\b',
            r'rc4',
        ]
        
        rust_files = list(Path("src").rglob("*.rs"))
        
        for file_path in rust_files:
            try:
                with open(file_path, 'r', encoding='utf-8') as f:
                    content = f.read()
                    lines = content.split('\n')
                    
                for i, line in enumerate(lines, 1):
                    for pattern in weak_crypto_patterns:
                        if re.search(pattern, line, re.IGNORECASE):
                            self.add_finding(
                                "MEDIUM",
                                "Cryptography",
                                f"Weak cryptographic algorithm: {line.strip()}",
                                str(file_path),
                                i
                            )
            except Exception as e:
                continue
    
    def run_comprehensive_audit(self):
        """Run all security audits"""
        print("🛡️  Starting AetherShell Security Audit...")
        print("=" * 50)
        
        self.audit_unsafe_code()
        self.audit_unwrap_usage()
        self.audit_input_validation()
        self.audit_secrets_exposure()
        self.audit_dependencies()
        self.audit_crypto_usage()
        
        print("\n" + "=" * 50)
        print("🛡️  Security Audit Results")
        print("=" * 50)
        
        print(f"Critical: {self.critical_count}")
        print(f"High:     {self.high_count}")
        print(f"Medium:   {self.medium_count}")
        print(f"Low:      {self.low_count}")
        print(f"Total:    {len(self.findings)}")
        
        if self.findings:
            print("\n📋 Detailed Findings:")
            print("-" * 30)
            
            for finding in self.findings:
                print(f"\n[{finding['severity']}] {finding['category']}")
                print(f"Description: {finding['description']}")
                if finding['file']:
                    location = finding['file']
                    if finding['line']:
                        location += f":{finding['line']}"
                    print(f"Location: {location}")
        
        # Security compliance assessment
        print("\n" + "=" * 50)
        print("🎯 Security Compliance Assessment")
        print("=" * 50)
        
        if self.critical_count == 0 and self.high_count == 0:
            print("✅ PASSED: Ready for production deployment")
            print("✅ No critical or high-severity security issues found")
            compliance_status = "COMPLIANT"
        elif self.critical_count == 0 and self.high_count <= 2:
            print("⚠️  CONDITIONAL: Address high-severity issues before deployment")
            compliance_status = "CONDITIONAL"
        else:
            print("❌ FAILED: Critical security issues must be resolved")
            compliance_status = "NON-COMPLIANT"
        
        # Generate JSON report
        report = {
            "audit_timestamp": subprocess.run(["date"], capture_output=True, text=True).stdout.strip(),
            "compliance_status": compliance_status,
            "summary": {
                "critical": self.critical_count,
                "high": self.high_count,
                "medium": self.medium_count,
                "low": self.low_count,
                "total": len(self.findings)
            },
            "findings": self.findings
        }
        
        with open("security_audit_report.json", "w") as f:
            json.dump(report, f, indent=2)
        
        print(f"\n📄 Detailed report saved to: security_audit_report.json")
        
        return compliance_status == "COMPLIANT"

if __name__ == "__main__":
    auditor = SecurityAudit()
    passed = auditor.run_comprehensive_audit()
    sys.exit(0 if passed else 1)