package fr.s1pierro.rustcraft;

import android.app.NativeActivity;
import android.content.ClipData;
import android.content.ActivityNotFoundException;
import android.content.Intent;
import android.net.Uri;
import android.widget.Toast;

/**
 * NativeActivity ne transmet pas onActivityResult au code natif : cette classe ouvre
 * le sélecteur de fichiers d'Android et passe les documents choisis à Rust.
 */
public class Activite extends NativeActivity {
    /** Selecteur::Importer : un fichier 3D s'accompagne de son .mtl et de ses images. */
    private static final int IMPORTER = 2;
    /** Selecteur::Image : une seule image, pour un matériau. */
    private static final int IMAGE = 3;

    /** Appelée depuis le fil de rendu. La demande revient avec le résultat. */
    public void choisir(final int demande) {
        runOnUiThread(new Runnable() {
            @Override
            public void run() {
                Intent i = new Intent(Intent.ACTION_OPEN_DOCUMENT);
                i.addCategory(Intent.CATEGORY_OPENABLE);
                // .gda et .obj n'ont pas de type reconnu partout
                i.setType(demande == IMAGE ? "image/*" : "*/*");
                i.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, demande == IMPORTER);
                try {
                    startActivityForResult(i, demande);
                } catch (ActivityNotFoundException e) {
                    Toast.makeText(Activite.this, "Aucun sélecteur de fichiers", Toast.LENGTH_LONG).show();
                }
            }
        });
    }

    @Override
    protected void onActivityResult(int demande, int resultat, Intent donnees) {
        super.onActivityResult(demande, resultat, donnees);
        if (resultat != RESULT_OK || donnees == null) {
            return;
        }
        ClipData l = donnees.getClipData();
        if (l != null && l.getItemCount() > 0) {
            int n = l.getItemCount();
            for (int k = 0; k < n; k++) {
                recu(l.getItemAt(k).getUri(), demande, n - 1 - k);
            }
        } else if (donnees.getData() != null) {
            recu(donnees.getData(), demande, 0);
        }
    }

    /** Enregistrée par ANativeActivity_onCreate. */
    private native void recu(Uri uri, int demande, int reste);
}
