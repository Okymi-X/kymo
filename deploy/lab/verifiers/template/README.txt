A Verifier definition. Copy this directory, give it a name, and edit it:

  cp -r /etc/kymo-lab/verifiers/template /etc/kymo-lab/verifiers/my-challenge

  verifier.json   the checks: id, the title players see, the language (bash, python or exec),
                  the file and function to run, an optional health function, a timeout
  checks.sh       check functions in bash
  checks.py       check functions in Python (standard library only)
  keys/           the SSH private key the checks use to reach the machine (never put it in
                  the machine image; only its public half belongs there)

Then name the directory in the image's manifest ("verifier": { "definition": "my-challenge" })
and test it against a running machine, with no player:

  kymo-lab check
  kymo-lab verifier test /etc/kymo-lab/verifiers/my-challenge --target 10.77.0.42 --ports 22,80

A fresh machine should show every check as failed, with its health function passing. After you
apply the intended fixes it should show fixed. The answer of a function is its exit status:
0 fixed, 1 the weakness is still there, 2 broken (a crash or a timeout counts as broken).
