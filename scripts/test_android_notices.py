import unittest
from android_notices import compare_graph

class AndroidNoticeTests(unittest.TestCase):
    def test_matching_coordinates_ignore_order_and_duplicate_variants(self):
        row={'group':'example','name':'library','version':'1'}
        compare_graph({'packages':[{'package':'example:library:1'}]},[row,row])
    def test_new_or_removed_runtime_dependency_blocks_stale_notice_bundle(self):
        reviewed={'packages':[{'package':'example:library:1'}]}
        for actual in [[],[{'group':'example','name':'library','version':'2'}]]:
            with self.assertRaisesRegex(ValueError,'stale'):compare_graph(reviewed,actual)

if __name__=='__main__':unittest.main()
