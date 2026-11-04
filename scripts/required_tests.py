"""Execute named CI suites; a required test may not pass by being skipped."""
import argparse
import sys
import unittest


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('tests',nargs='+',help='Importable test module, class or case names')
    args=parser.parse_args()
    suite=unittest.defaultTestLoader.loadTestsFromNames(args.tests)
    result=unittest.TextTestRunner(verbosity=2).run(suite)
    complete=result.testsRun>0 and not result.skipped
    if not complete:
        print('Required tests did not all execute: '+str(result.testsRun)+' run, '+str(len(result.skipped))+' skipped',file=sys.stderr)
    return 0 if result.wasSuccessful() and complete else 1


if __name__=='__main__':raise SystemExit(main())
