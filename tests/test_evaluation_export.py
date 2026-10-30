import importlib.util, unittest
from pathlib import Path
spec=importlib.util.spec_from_file_location('analysis',Path(__file__).parents[1]/'scripts/analyze-evaluation.py')
analysis=importlib.util.module_from_spec(spec);spec.loader.exec_module(analysis)

class EvaluationExportTests(unittest.TestCase):
    def data(self,simulated=False):
        def topic(condition,points):
            return {'id':condition,'name':'=UNTRUSTED()','condition':condition,'simulated':simulated,'prior_knowledge':'Reported none','items':[{'id':'f','form':'familiar','max_points':5,'review_provenance':'Pending review'},{'id':'c','form':'changed','max_points':5,'review_provenance':'Pending review'}],'lesson':{'completed_at':'2026-01-01T00:00:00Z','assessment_due_at':'2026-01-08T00:00:00Z','review_exclusion_at':'2026-01-06T00:00:00Z'},'responses':{'submitted_at':'2026-01-08T00:00:00Z','outside_help':False,'elapsed_seconds':300},'scores':[{'reviewer':'Self-score fixture','blinded':False,'scores':[{'item_id':'f','points':points},{'item_id':'c','points':points}]}],'events':[],'evidence':{'usage':[]},'recorded_exposures_in_exclusion_window':0}
        return {'protocol':'personal-comparison-v1','simulated':simulated,'pairs':[{'id':'pair','simulated':simulated,'registration':{'time_allowance_minutes':30,'decision_criterion':'Pre-registered fixture, not actual outcomes'},'versions':{},'topics':[topic('Study Harness',4),topic('baseline',3)]}]}
    def test_simulated_scores_cannot_become_actual_paired_results(self):
        topics,pairs,summary=analysis.analyze(self.data(True));self.assertFalse(pairs[0]['actual_scored_pair']);self.assertIsNone(summary['mean_raw_difference_percentage_points']);self.assertEqual(summary['actual_scored_pairs'],0)
    def test_raw_paired_differences_preserve_help_and_timing(self):
        d=self.data();topics,pairs,summary=analysis.analyze(d);self.assertEqual(topics[0]['actual_delay_hours'],168);self.assertEqual(pairs[0]['tool_minus_baseline_percentage_points'],20);d['pairs'][0]['topics'][0]['responses']['outside_help']=True;self.assertEqual(analysis.analyze(d)[2]['reported_unaided_pairs_without_recorded_window_exposure'],0)
    def test_free_text_is_inert_but_negative_numbers_remain_numeric(self):
        self.assertTrue(analysis.safe_cell(' \t=EXEC()').startswith("'"));self.assertEqual(analysis.safe_cell(-10),-10)
    def test_missing_simulation_flag_is_rejected(self):
        d=self.data();del d['pairs'][0]['simulated'];self.assertRaises(ValueError,analysis.analyze,d)

if __name__=='__main__':unittest.main()
