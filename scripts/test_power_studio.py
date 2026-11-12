import importlib.util
from pathlib import Path
import tempfile
import unittest
import numpy as np

SPEC = importlib.util.spec_from_file_location('builder', Path(__file__).with_name('build_power_studio.py'))
builder = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(builder)


class PowerStudioTests(unittest.TestCase):
    def test_changed_source_is_refused_before_output_creation(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)
            (root/'source.wav').write_bytes(b'changed')
            recipe=dict(source_hashes={'source.wav':'0'*64}, voices=[])
            with self.assertRaises(ValueError):
                builder.build(root,root/'new.shrkit',recipe)
            self.assertFalse((root/'new.shrkit').exists())

    def test_existing_package_is_never_replaced(self):
        with tempfile.TemporaryDirectory(suffix='.shrkit') as d:
            root=Path(d)
            (root/'keep').write_text('musician data')
            with self.assertRaises(ValueError):builder.build(root,root,{})
            self.assertEqual((root/'keep').read_text(),'musician data')

    def test_escaping_source_is_refused(self):
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaises(ValueError):builder.checked_path(Path(d),'../outside')

    def test_synthesis_is_repeatable_with_distinct_takes(self):
        for name in ['clap','splash','tambourine','cowbell']:
            v=dict(id=name,note=55,tail=.5,kind='synthetic')
            first=builder.synthesized(v,2,1)
            np.testing.assert_array_equal(first,builder.synthesized(v,2,1))
            self.assertFalse(np.array_equal(first,builder.synthesized(v,2,2)))

    def test_processing_is_finite_faded_and_preserves_velocity_order(self):
        v=dict(id='clap',note=39,tail=.6,kind='synthetic')
        arrays=[builder.synthesized(v,layer,1) for layer in range(3)]
        result=builder.process_voice(arrays,v)
        energies=[]
        for x in result:
            self.assertTrue(np.isfinite(x).all())
            self.assertLessEqual(np.max(np.abs(x)),10**(-3/20)+1e-10)
            np.testing.assert_array_equal(x[-1],[0.,0.])
            energies.append(np.mean(x*x))
        self.assertLess(energies[0],energies[1]);self.assertLess(energies[1],energies[2])

    def test_locked_recipe_has_distinct_map_and_complete_source_coverage(self):
        import json
        recipe=json.loads(builder.RECIPE.read_text())
        self.assertEqual(len({v['note'] for v in recipe['voices']}),30)
        for v in recipe['voices']:
            for s in v['samples']:
                self.assertIn(s['source'],recipe['source_hashes'])
                self.assertEqual(len(recipe['source_hashes'][s['source']]),64)

if __name__=='__main__':unittest.main()
