package chat.cursus.calls

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import chat.cursus.CursusApplication
import chat.cursus.MainActivity

/** Private notification entry point; exported launcher intents cannot answer calls. */
class AnswerCallActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val calls = (application as CursusApplication).model.calls
        // After process death, merely open the app; never arm a future call.
        calls.answerRequested = calls.state.active?.phase == "incoming"
        calls.expanded = true
        startActivity(Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP))
        finish()
    }
}
