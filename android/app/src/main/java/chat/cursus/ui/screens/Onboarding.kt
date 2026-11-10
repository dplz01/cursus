package chat.cursus.ui.screens

import androidx.compose.runtime.remember
import kotlinx.coroutines.launch
import androidx.compose.material3.Icon
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CheckboxDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import chat.cursus.AppModel
import chat.cursus.R
import chat.cursus.ui.components.CursusCard
import chat.cursus.ui.components.CursusField
import chat.cursus.ui.components.DangerButton
import chat.cursus.ui.components.PrimaryButton
import chat.cursus.ui.components.SecondaryButton
import chat.cursus.ui.components.clickableNoRipple
import chat.cursus.ui.theme.Cursus

@Composable
private fun OnboardingPage(content: @Composable () -> Unit) {
    Box(
        Modifier.fillMaxSize().background(Cursus.colors.bgApp).safeDrawingPadding().imePadding()
            .verticalScroll(rememberScrollState()),
        contentAlignment = Alignment.TopCenter,
    ) {
        Column(
            Modifier.widthIn(max = 440.dp).fillMaxWidth().padding(horizontal = 16.dp, vertical = 32.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Image(painterResource(R.mipmap.ic_launcher), null, Modifier.size(72.dp))
            content()
        }
    }
}

@Composable
private fun RememberRow(checked: Boolean, onChange: (Boolean) -> Unit) {
    val c = Cursus.colors
    Row(verticalAlignment = Alignment.CenterVertically) {
        Checkbox(checked, onChange, colors = CheckboxDefaults.colors(checkedColor = c.accent, checkmarkColor = c.accentOn))
        Column {
            Text("Remember password on this phone", style = Cursus.type.body, color = c.textPrimary)
            Text("Kept in the Android Keystore; unlock without typing it.", style = Cursus.type.meta, color = c.textSecondary)
        }
    }
}

@Composable
fun WelcomeScreen(model: AppModel) {
    val c = Cursus.colors
    var name by rememberSaveable { mutableStateOf("") }
    var password by rememberSaveable { mutableStateOf("") }
    var confirm by rememberSaveable { mutableStateOf("") }
    var remember by rememberSaveable { mutableStateOf(true) }
    val mismatch = confirm.isNotEmpty() && confirm != password
    val ready = name.isNotBlank() && password.length >= 4 && password == confirm && !model.busy

    OnboardingPage {
        Text("Welcome to Cursus", style = Cursus.type.title, color = c.textPrimary)
        Text(
            "Private messaging on SimpleX: no phone number, no user IDs. Everything on this phone is encrypted with your password.",
            style = Cursus.type.meta, color = c.textSecondary, textAlign = TextAlign.Center,
        )
        CursusCard {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text("Your name", style = Cursus.type.meta, color = c.textMuted)
                CursusField(name, { name = it }, "Shown to your contacts")
                Text("Password", style = Cursus.type.meta, color = c.textMuted)
                CursusField(password, { password = it }, "At least 4 characters", password = true)
                CursusField(confirm, { confirm = it }, "Repeat password", password = true)
                if (mismatch) Text("Passwords don't match", style = Cursus.type.meta, color = c.danger)
                Text(
                    "There is no way to recover a forgotten password: without it, the chats on this phone are lost.",
                    style = Cursus.type.meta, color = c.textSecondary,
                )
                RememberRow(remember) { remember = it }
            }
        }
        PrimaryButton(
            if (model.busy) "Creating…" else "Create profile",
            { model.createAccount(name.trim(), password, remember) },
            Modifier.fillMaxWidth(),
            enabled = ready,
        )
        if (model.busy) CircularProgressIndicator(color = c.accent)
    }
}

@Composable
fun UnlockScreen(model: AppModel) {
    val c = Cursus.colors
    var password by rememberSaveable { mutableStateOf("") }
    var remember by rememberSaveable { mutableStateOf(model.rememberedPassword) }
    var confirmWipe by rememberSaveable { mutableStateOf(false) }

    OnboardingPage {
        Text("Unlock Cursus", style = Cursus.type.title, color = c.textPrimary)
        model.info?.displayName?.takeIf { it.isNotBlank() }?.let {
            Text(it, style = Cursus.type.body, color = c.textSecondary)
        }
        CursusCard {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                CursusField(password, { password = it }, "Password", password = true)
                RememberRow(remember) { remember = it }
            }
        }
        PrimaryButton(
            if (model.busy) "Unlocking…" else "Unlock",
            { model.unlock(password, remember) },
            Modifier.fillMaxWidth(),
            enabled = password.isNotEmpty() && !model.busy,
        )
        if (model.busy) CircularProgressIndicator(color = c.accent)
        Spacer(Modifier.height(24.dp))
        if (!confirmWipe) {
            Text(
                "Forgot your password?",
                style = Cursus.type.meta, color = c.textMuted,
                modifier = Modifier.padding(8.dp).then(Modifier.clickableNoRipple { confirmWipe = true }),
            )
        } else {
            CursusCard {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text("Start over?", style = Cursus.type.body, color = c.textPrimary, fontWeight = FontWeight.Medium)
                    Text(
                        "This deletes your profile, contacts and messages on this phone. It cannot be undone.",
                        style = Cursus.type.meta, color = c.textSecondary,
                    )
                    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        SecondaryButton("Cancel", { confirmWipe = false }, Modifier.weight(1f))
                        DangerButton("Delete everything", { model.wipeAll() }, Modifier.weight(1f))
                    }
                }
            }
        }
    }
}

@Composable
fun StartingScreen() {
    Box(Modifier.fillMaxSize().background(Cursus.colors.bgApp), contentAlignment = Alignment.Center) {
        CircularProgressIndicator(color = Cursus.colors.accent)
    }
}

@Composable
fun FailedScreen(message: String) {
    OnboardingPage {
        Text("Cursus couldn't start", style = Cursus.type.title, color = Cursus.colors.textPrimary)
        Text(message, style = Cursus.type.meta, color = Cursus.colors.textSecondary)
    }
}


/** Linux app lock screen: the chats stay hidden until the database password
 *  is entered again; messages keep arriving meanwhile. */
@Composable
fun AppLockScreen(model: AppModel) {
    val c = Cursus.colors
    val scope = androidx.compose.runtime.rememberCoroutineScope()
    var password by remember { mutableStateOf("") }
    var checking by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var retryAt by remember { mutableStateOf(0L) }
    val submit = submit@{
        if (checking || password.isEmpty()) return@submit
        val now = android.os.SystemClock.elapsedRealtime()
        if (now < retryAt) {
            error = "Too many attempts. Try again in ${(retryAt - now) / 1000 + 1} s."
            return@submit
        }
        checking = true
        scope.launch {
            val result = model.unlockApp(password)
            checking = false
            if (result != null) {
                val (message, wait) = result
                password = ""
                retryAt = android.os.SystemClock.elapsedRealtime() + wait * 1000
                error = if (wait > 0) "$message Try again in $wait s." else message
            }
        }
    }
    OnboardingPage {
        Icon(painterResource(R.drawable.ic_lock), null, Modifier.size(40.dp), tint = c.accent)
        Text("Cursus is locked", style = Cursus.type.title, color = c.textPrimary)
        Text("Enter your database password to continue.", style = Cursus.type.body, color = c.textSecondary)
        CursusCard {
            CursusField(password, { password = it; error = null }, "Database password", password = true)
        }
        error?.let { Text(it, style = Cursus.type.meta, color = c.danger) }
        PrimaryButton(
            if (checking) "Checking…" else "Unlock",
            { submit() },
            Modifier.fillMaxWidth(),
            enabled = password.isNotEmpty() && !checking,
        )
    }
}
