// Jenkins reference pipeline for plsql-intelligence change-impact gating
// (PLSQL-CICD-019 / oracle-xi1c).
//
// Mirrors the GitHub Actions + GitLab CI examples in this directory:
// runs the predict → plan → gate cycle on every PR-bound commit and
// posts a single self-editing comment on the upstream PR.
//
// Notes:
//   - Requires the `plsql cicd` CLI bundle on the Jenkins agent
//     (or a Docker exec stage that pulls it).
//   - `withCredentials` reads PLSQL_GH_TOKEN / PLSQL_GL_TOKEN from the
//     Jenkins credentials store; the env vars never appear in build
//     logs.
//   - The pipeline assumes a Multibranch Pipeline job so
//     `env.CHANGE_TARGET` and `env.CHANGE_ID` are populated for PRs.

pipeline {
    agent any

    options {
        timestamps()
        ansiColor('xterm')
        timeout(time: 30, unit: 'MINUTES')
    }

    stages {
        stage('Checkout') {
            steps {
                checkout scm
            }
        }

        stage('Compute changeset') {
            when { changeRequest() }
            steps {
                sh '''
                    git fetch --no-tags origin ${CHANGE_TARGET}
                    git diff --unified=3 origin/${CHANGE_TARGET}...HEAD -- '*.sql' '*.pks' '*.pkb' \\
                        > target/plsql-changeset.diff
                '''
            }
        }

        stage('plsql cicd predict') {
            when { changeRequest() }
            steps {
                sh 'plsql cicd predict --changeset target/plsql-changeset.diff --robot-json > target/predict.json'
                archiveArtifacts artifacts: 'target/predict.json', fingerprint: true
            }
        }

        stage('plsql cicd plan') {
            when { changeRequest() }
            steps {
                sh 'plsql cicd plan --changeset target/plsql-changeset.diff --robot-json > target/plan.json'
                archiveArtifacts artifacts: 'target/plan.json', fingerprint: true
            }
        }

        stage('plsql cicd gate') {
            when { changeRequest() }
            steps {
                sh 'plsql cicd gate --predict target/predict.json --policy .plsql-cicd-policy.toml --pr-comment-json > target/gate.json'
                archiveArtifacts artifacts: 'target/gate.json', fingerprint: true
            }
        }

        stage('Post PR comment (GitHub)') {
            when {
                allOf {
                    changeRequest()
                    environment name: 'PLSQL_PR_PLATFORM', value: 'github'
                }
            }
            steps {
                withCredentials([string(credentialsId: 'plsql-gh-token', variable: 'PLSQL_GH_TOKEN')]) {
                    sh '''
                        plsql cicd post-pr-comment \\
                            --envelope target/gate.json \\
                            --platform github \\
                            --owner "${PLSQL_PR_OWNER}" \\
                            --repository "${PLSQL_PR_REPO}" \\
                            --pull-request "${CHANGE_ID}"
                    '''
                }
            }
        }

        stage('Post MR comment (GitLab)') {
            when {
                allOf {
                    changeRequest()
                    environment name: 'PLSQL_PR_PLATFORM', value: 'gitlab'
                }
            }
            steps {
                withCredentials([string(credentialsId: 'plsql-gl-token', variable: 'PLSQL_GL_TOKEN')]) {
                    sh '''
                        plsql cicd post-pr-comment \\
                            --envelope target/gate.json \\
                            --platform gitlab \\
                            --owner "${PLSQL_PR_OWNER}" \\
                            --repository "${PLSQL_PR_REPO}" \\
                            --pull-request "${CHANGE_ID}"
                    '''
                }
            }
        }
    }

    post {
        failure {
            echo '[plsql-cicd] gate failed — see archived target/gate.json for the rule violations.'
        }
        success {
            echo '[plsql-cicd] gate passed.'
        }
    }
}
